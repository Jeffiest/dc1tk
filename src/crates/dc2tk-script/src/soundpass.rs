//! The sound pass of presenting a script: names the entry number of play_se(id, n) and its kin by
//! the bank the id was read from (the system bank, a scene bank, a bank load_sound loaded, the
//! event port's latest bank), following ids through variables, globals, parameters and helpers'
//! out-parameters across a program's functions.
//!
//! The event port is followed like a variable every function shares: a function starts with the
//! bank its callers leave there, a call leaves the bank the function leaves, and an event the
//! engine starts finds the port's resting bank, the one every event that loads another bank puts
//! back before it ends (a dungeon floor's battle music, EV_200_000).

use std::collections::{BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;

use crate::node::Node;
use crate::sdk::{named_literal, Context};

/// Natives that take a bank id and an entry of it, by the positions of the two.
fn sound_entry_args(name: &str) -> Option<(usize, usize)> {
    Some(match name {
        "play_se" | "stop_se" | "pause_se" | "set_se_volume" | "set_se_pan" | "set_se_pitch" | "get_se_status" => {
            (0, 1)
        }
        "play_actor_se" => (1, 2),
        "play_actor_se_after" => (2, 3),
        _ => return None,
    })
}

/// The natives the sound pass reads.
pub fn is_sound_native(name: &str) -> bool {
    sound_entry_args(name).is_some()
        || matches!(
            name,
            "load_sound"
                | "get_sound_id"
                | "get_system_sound_id"
                | "get_sound_port_bank"
                | "reset_sound_port"
                | "delete_sound_port"
        )
}

/// What the pass records for a variable that is written, or a parameter passed, something it cannot
/// follow; a variable seen with it is never settled.
pub const UNKNOWN_BANK: &str = "?";

/// One thing the event port may hold.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Held {
    /// The port as the function found it, while the pass does not know what its callers leave
    /// there: a function that ends with it leaves the port as it was.
    AtEntry,
    Bank(String),
}

/// What the event port may hold at a point of a function. The port always holds something, so
/// where paths meet it may hold what either left (`join`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PortValue {
    /// Something the pass cannot name; joined with anything it stays unknown.
    #[default]
    Unknown,
    /// One of these, where paths differ.
    OneOf(BTreeSet<Held>),
}

impl PortValue {
    fn at_entry() -> PortValue {
        PortValue::OneOf(BTreeSet::from([Held::AtEntry]))
    }

    fn bank(bank: &str) -> PortValue {
        PortValue::OneOf(BTreeSet::from([Held::Bank(bank.to_string())]))
    }

    /// The bank the port holds, when the pass knows it is that one.
    fn only_bank(&self) -> Option<&str> {
        match self {
            PortValue::OneOf(held) if held.len() == 1 => match held.first() {
                Some(Held::Bank(bank)) => Some(bank),
                _ => None,
            },
            _ => None,
        }
    }

    /// What the port may hold where a path with this value meets one with other.
    fn join(&self, other: &PortValue) -> PortValue {
        match (self, other) {
            (PortValue::OneOf(a), PortValue::OneOf(b)) => PortValue::OneOf(a.union(b).cloned().collect()),
            _ => PortValue::Unknown,
        }
    }

    /// The join of values; None for none.
    fn joined<'a>(values: impl IntoIterator<Item = &'a PortValue>) -> Option<PortValue> {
        values.into_iter().fold(None, |joined: Option<PortValue>, value| match joined {
            None => Some(value.clone()),
            Some(joined) => Some(joined.join(value)),
        })
    }

    /// The port after a call to a function that leaves this value there (left, None when the
    /// pass saw it return nowhere), when it held before: AtEntry stands for what it held before.
    fn after_call(left: Option<&PortValue>, before: &PortValue) -> PortValue {
        match left {
            Some(PortValue::OneOf(held)) => {
                let banks = PortValue::OneOf(held.iter().filter(|held| **held != Held::AtEntry).cloned().collect());
                if held.contains(&Held::AtEntry) {
                    banks.join(before)
                } else {
                    banks
                }
            }
            _ => PortValue::Unknown,
        }
    }

    /// The banks the port may hold besides what it held at entry; None when it may hold
    /// something the pass cannot name.
    fn banks(&self) -> Option<BTreeSet<&str>> {
        match self {
            PortValue::OneOf(held) => Some(
                held.iter()
                    .filter_map(|held| match held {
                        Held::Bank(bank) => Some(bank.as_str()),
                        Held::AtEntry => None,
                    })
                    .collect(),
            ),
            PortValue::Unknown => None,
        }
    }
}

/// What the variables a function can see hold, and the event port.
#[derive(Clone, Debug, Default)]
pub struct Banks {
    /// The bank a variable (a local, a parameter, a global) holds the id of; a conditional bank
    /// (cond_parts) where that depends on an if the pass can test again.
    pub vars: HashMap<String, String>,
    port: PortValue,
}

impl Banks {
    /// What a call passes on the event port: its bank, or UNKNOWN_BANK.
    fn port_passed(&self) -> String {
        self.port.only_bank().map_or_else(|| UNKNOWN_BANK.into(), str::to_string)
    }
}

/// The kind of sound entry play_se plays from a bank id, by where the id came from.
fn scene_bank(bank: &Node) -> Option<&'static str> {
    match bank {
        Node::Str(name) => match name.as_str() {
            "SE_BANK_BASE" | "SE_BANK_BASE_2" | "SE_BANK_BASE_3" => Some("base_se"),
            "SE_BANK_BATTLE" => Some("battle_se"),
            _ => None,
        },
        Node::Int(1..=3) => Some("base_se"),
        Node::Int(4) => Some("battle_se"),
        _ => None,
    }
}

fn assigned_var(node: &Node) -> Option<String> {
    match node.kind() {
        "var" => Some(node.text("name").to_string()),
        "addr" if node["target"].kind() == "var" => Some(node["target"].text("name").to_string()),
        _ => None,
    }
}

fn bank_kind(stem: &str) -> String {
    format!("se_{}", stem.to_lowercase())
}

/// An int literal's value (-1 included), or an int constant's.
fn literal_value(node: &Node, found: &Context) -> Option<i64> {
    match node.kind() {
        "int" => node["value"].as_i64(),
        "unary" if node.text("op") == "-" => literal_value(&node["operand"], found).map(|value| -value),
        "var" => {
            let value = found.constants.get(node.text("name"))?;
            if value.kind() == "int" {
                value["value"].as_i64()
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The bank load_sound loads: a path names its bank by file stem ("snd2/sp/sp_000.snd" is SP_000),
/// a number is a sound set whose event bank it loads (sound_set_banks, from the definition set).
fn loaded_bank(node: &Node, found: &Context) -> Option<String> {
    let args = node.list("args");
    let source = if args.len() > 2 || args[1].kind() != "addr" { &args[1] } else { &args[0] };
    if source.kind() == "string" {
        let path = source.text("text").replace('\\', "/");
        let file = path.rsplit('/').next().unwrap_or("");
        let stem = file.split('.').next().unwrap_or("");
        return (!stem.is_empty()).then(|| bank_kind(stem));
    }
    let number = literal_value(source, found)?;
    found.sound_set_banks.get(&number).filter(|bank| !bank.is_empty()).map(|bank| bank_kind(bank))
}

/// (function, parameter position).
pub type ParamKey = (String, usize);

/// What the pass settles across a program's functions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Known {
    pub globals: HashMap<String, String>,
    pub params: HashMap<ParamKey, String>,
    pub outs: HashMap<ParamKey, String>,
    /// The event port's resting bank: what it holds when the engine starts an event.
    pub port: Option<String>,
    /// The bank on the event port when a function starts, where every way in agrees.
    pub port_in: HashMap<String, String>,
    /// What a function leaves on the event port where it returns.
    pub port_out: HashMap<String, PortValue>,
}

/// What one round of the pass sees: every bank written to globals, passed for parameters and
/// written through out-parameters; the port's bank where each function is called (or started as
/// an event).
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub globals: IndexMap<String, BTreeSet<String>>,
    pub params: IndexMap<ParamKey, BTreeSet<String>>,
    pub outs: IndexMap<ParamKey, BTreeSet<String>>,
    pub port_in: IndexMap<String, BTreeSet<String>>,
}

/// What the scripts that include a library pass it: a bank for each parameter and global they
/// agree on, and the event port's bank where each function starts (UNKNOWN_BANK where they do not).
#[derive(Clone, Debug, Default)]
pub struct Seed {
    pub params: IndexMap<ParamKey, String>,
    pub globals: IndexMap<String, String>,
    pub port_in: IndexMap<String, String>,
}

/// What a script shows about the library code it includes: the banks it passes each parameter of
/// the hidden functions, the bank each global they use holds, and the event port's bank where it
/// calls or starts each of them.
#[derive(Clone, Debug, Default)]
pub struct Passed {
    pub params: IndexMap<ParamKey, BTreeSet<String>>,
    pub globals: IndexMap<String, BTreeSet<String>>,
    pub port_in: IndexMap<String, BTreeSet<String>>,
}

/// The sound pass's state across scripts: the seed for a library, where to put what a script
/// passes its libraries, and included functions prepared once per key.
#[derive(Default)]
pub struct Sounds {
    /// The functions the engine starts (the script's programs and main); None for a library,
    /// whose functions' starting port comes from the scripts that include it (seed).
    pub entries: Option<HashSet<String>>,
    pub seed: Option<Seed>,
    /// Whether to record what the script passes its libraries, into out.
    pub want_out: bool,
    pub out: Option<Passed>,
    pub cache: HashMap<String, Node>,
    pub keys: HashMap<String, String>,
}

/// The names a function's own variables and pointer parameters have, and what the pass settled.
pub struct Flow<'a> {
    pub function: String,
    pub locals: HashSet<String>,
    pub pointers: HashMap<String, usize>,
    pub known: &'a Known,
    /// Whether the function reads the dungeon battle music's volume (get_battle_bgm_volume): the
    /// event port's bank it reads is the one the engine plays the battle music from.
    pub battle_music: bool,
}

/// The kind of an entry of the event port's bank when the pass does not know the bank but the
/// function handles the dungeon battle music, which the engine plays from entry 0 (enum
/// se_event_port).
const EVENT_PORT_KIND: &str = "se_event_port";

fn calls_native(node: &Node, name: &str) -> bool {
    match node {
        Node::List(items) => items.iter().any(|item| calls_native(item, name)),
        Node::Map(map) => {
            (node.kind() == "native" && node.text("name") == name)
                || map.values().any(|value| calls_native(value, name))
        }
        _ => false,
    }
}

pub fn sound_flow<'a>(function: &Node, known: &'a Known) -> Flow<'a> {
    let locals = function
        .list("params")
        .iter()
        .chain(function.list("locals"))
        .map(|variable| variable.text("name").to_string())
        .collect();
    let pointers = function
        .list("params")
        .iter()
        .enumerate()
        .filter(|(_, param)| param.flag("array"))
        .map(|(position, param)| (param.text("name").to_string(), position))
        .collect();
    let battle_music = calls_native(&function["body"], "get_battle_bgm_volume");
    Flow { function: function.text("name").to_string(), locals, pointers, known, battle_music }
}

/// The banks a function starts with: its globals and parameters the program pass settled, and the
/// event port's.
pub fn sound_banks(function: &Node, known: &Known) -> Banks {
    let locals: HashSet<&str> =
        function.list("params").iter().chain(function.list("locals")).map(|variable| variable.text("name")).collect();
    let mut vars: HashMap<String, String> = known
        .globals
        .iter()
        .filter(|(name, _)| !locals.contains(name.as_str()))
        .map(|(name, kind)| (name.clone(), kind.clone()))
        .collect();
    let name = function.text("name");
    for (position, param) in function.list("params").iter().enumerate() {
        if let Some(kind) = known.params.get(&(name.to_string(), position)) {
            vars.insert(param.text("name").to_string(), kind.clone());
        }
    }
    let port = known.port_in.get(name).map_or_else(PortValue::at_entry, |bank| PortValue::bank(bank));
    Banks { vars, port }
}

fn record_bank(name: &str, kind: &str, flow: Option<&Flow>, info: &mut Option<&mut Info>) {
    let (Some(flow), Some(info)) = (flow, info.as_deref_mut()) else {
        return;
    };
    if let Some(&position) = flow.pointers.get(name) {
        info.outs.entry((flow.function.clone(), position)).or_default().insert(kind.to_string());
    } else if !flow.locals.contains(name) {
        info.globals.entry(name.to_string()).or_default().insert(kind.to_string());
    }
}

/// Marks a bank that depends on a condition: a variable that holds one bank after an if's then
/// path and another after its else path, written COND vars KEY then KEY else ("" for no bank),
/// with the condition's variables and text. A later if with the same condition, while none of
/// its variables was written, takes each path's bank (if (holed == 1) after the hole-in fanfare).
const COND: char = '\u{1}';
const COND_KEY: char = '\u{2}';

/// A conditional bank's condition variables, condition text, and then and else banks.
fn cond_parts(value: &str) -> Option<(&str, &str, &str, &str)> {
    let rest = value.strip_prefix(COND)?;
    let mut parts = rest.split(COND_KEY);
    Some((parts.next()?, parts.next()?, parts.next()?, parts.next()?))
}

/// A bank a variable holds whichever way the program went, as play_se and calls read it.
fn plain(value: Option<&String>) -> Option<&String> {
    value.filter(|value| !value.starts_with(COND))
}

/// The text of a condition the pass can test again (no calls or assignments), and its variables.
fn condition_key(cond: &Node) -> Option<(String, String)> {
    fn pure(node: &Node, vars: &mut BTreeSet<String>) -> bool {
        match node {
            Node::List(items) => items.iter().all(|item| pure(item, vars)),
            Node::Map(map) => {
                if matches!(node.kind(), "call" | "native" | "assign") {
                    return false;
                }
                if node.kind() == "var" {
                    vars.insert(node.text("name").to_string());
                }
                map.values().all(|value| pure(value, vars))
            }
            _ => true,
        }
    }
    let mut vars = BTreeSet::new();
    if !pure(cond, &mut vars) {
        return None;
    }
    let (key, _) = crate::source::expression(cond).ok()?;
    Some((vars.into_iter().collect::<Vec<_>>().join(","), key))
}

/// Forgets the conditional banks whose condition reads a variable just written.
fn invalidate(banks: &mut HashMap<String, String>, written: &str) {
    banks.retain(|_, value| cond_parts(value).is_none_or(|(vars, ..)| !vars.split(',').any(|var| var == written)));
}

/// Two paths' values for a variable merged, where they differ: a conditional bank on the if's
/// condition, or merged into the conditional bank one of them already holds. None for no bank.
fn merged_cond(then: Option<&String>, otherwise: Option<&String>, cond: Option<&(String, String)>) -> Option<String> {
    let same = |a: &str, b: &str| if a == b { a.to_string() } else { String::new() };
    let value = |vars: &str, key: &str, then: &str, otherwise: &str| {
        (!then.is_empty() || !otherwise.is_empty())
            .then(|| format!("{COND}{vars}{COND_KEY}{key}{COND_KEY}{then}{COND_KEY}{otherwise}"))
    };
    let (a, b) = (then.map_or("", String::as_str), otherwise.map_or("", String::as_str));
    match (cond_parts(a), cond_parts(b)) {
        (None, None) => {
            let (vars, key) = cond?;
            value(vars, key, a, b)
        }
        (Some((vars, key, t, e)), None) => value(vars, key, &same(t, b), &same(e, b)),
        (None, Some((vars, key, t, e))) => value(vars, key, &same(a, t), &same(a, e)),
        (Some((vars, key, t1, e1)), Some((_, key2, t2, e2))) if key == key2 => {
            value(vars, key, &same(t1, t2), &same(e1, e2))
        }
        _ => None,
    }
}

fn set_bank(
    banks: &mut HashMap<String, String>,
    name: &str,
    kind: &str,
    flow: Option<&Flow>,
    info: &mut Option<&mut Info>,
) {
    invalidate(banks, name);
    banks.insert(name.to_string(), kind.to_string());
    // a conditional bank is not one bank to a global or a caller
    record_bank(name, if kind.starts_with(COND) { UNKNOWN_BANK } else { kind }, flow, info);
}

/// A write of something other than a bank id: the variable no longer holds a bank here, and a
/// global, out-parameter or the event port written so is not always one bank.
fn forget_bank(
    banks: &mut HashMap<String, String>,
    name: Option<&str>,
    flow: Option<&Flow>,
    info: &mut Option<&mut Info>,
) {
    let Some(name) = name else {
        return;
    };
    banks.remove(name);
    invalidate(banks, name);
    record_bank(name, UNKNOWN_BANK, flow, info);
}

/// The pointer parameter p an assignment writes through (p[0] = ... or *p = ...).
fn pointer_target(node: &Node, flow: Option<&Flow>) -> Option<String> {
    let flow = flow?;
    if node.kind() == "index" && flow.pointers.contains_key(node.text("name")) {
        return Some(node.text("name").to_string());
    }
    if node.kind() == "deref"
        && node["operand"].kind() == "var"
        && flow.pointers.contains_key(node["operand"].text("name"))
    {
        return Some(node["operand"].text("name").to_string());
    }
    None
}

const ENDS_PATH: [&str; 3] = ["return", "break", "continue"];

/// Sets banks to what the paths that go on agree on: a variable one path forgets, or two paths give
/// different banks, is forgotten (or, after an if whose condition can be tested again, holds a
/// conditional bank); one only some paths set keeps that bank (on the other paths it was never a
/// bank id). A path whose statements end in a return, break or continue does not go on. cond: the
/// if's condition, with paths its then and else.
fn merge_banks(banks: &mut Banks, paths: Vec<(Banks, &[Node])>, cond: Option<&(String, String)>) {
    let going_on: Vec<Banks> = paths
        .into_iter()
        .filter(|(_, statements)| statements.last().is_none_or(|last| !ENDS_PATH.contains(&last.kind())))
        .map(|(path_banks, _)| path_banks)
        .collect();
    if going_on.is_empty() {
        return;
    }
    // the port always holds something: after paths that disagree, what either left
    banks.port = PortValue::joined(going_on.iter().map(|path_banks| &path_banks.port)).expect("a path goes on");
    // only an if whose two paths go on can tell them apart later
    let cond = cond.filter(|_| going_on.len() == 2);
    let before = std::mem::take(&mut banks.vars);
    let mut all: HashSet<&String> = before.keys().collect();
    for path_banks in &going_on {
        all.extend(path_banks.vars.keys());
    }
    for name in all {
        let kinds: HashSet<Option<&String>> = going_on
            .iter()
            .filter(|path_banks| path_banks.vars.contains_key(name) || before.contains_key(name))
            .map(|path_banks| path_banks.vars.get(name))
            .collect();
        if kinds.len() == 1 {
            if let Some(Some(kind)) = kinds.into_iter().next() {
                banks.vars.insert(name.clone(), kind.clone());
            }
        } else if going_on.len() == 2 {
            if let Some(value) = merged_cond(going_on[0].vars.get(name), going_on[1].vars.get(name), cond) {
                banks.vars.insert(name.clone(), value);
            }
        }
    }
}

/// Names the entries of the sound natives in a function's statements and follows the banks.
pub fn named_sounds(
    nodes: &[Node],
    found: &Context,
    banks: &mut Banks,
    flow: Option<&Flow>,
    mut info: Option<&mut Info>,
) -> Node {
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        let kind = node.kind();
        let mut node = node.clone();
        if kind == "if" {
            // each branch starts from the banks after the condition, and after the if a variable
            // keeps the bank every path that goes on agrees on
            let cond = sound_expression(&node["cond"], found, banks, flow, &mut info);
            let key = condition_key(&cond);
            node = node.with("cond", cond);
            let (mut then_banks, mut else_banks) = (banks.clone(), banks.clone());
            // a bank that depends on this same condition is known on each path
            if let Some((_, key)) = &key {
                for (name, value) in &banks.vars {
                    if let Some((_, _, then, otherwise)) = cond_parts(value).filter(|parts| parts.1 == key) {
                        for (path_banks, bank) in [(&mut then_banks, then), (&mut else_banks, otherwise)] {
                            if bank.is_empty() {
                                path_banks.vars.remove(name);
                            } else {
                                path_banks.vars.insert(name.clone(), bank.to_string());
                            }
                        }
                    }
                }
            }
            let then = named_sounds(node.list("then"), found, &mut then_banks, flow, info.as_deref_mut());
            let otherwise = match node["else"].as_array() {
                Some(items) => named_sounds(items, found, &mut else_banks, flow, info.as_deref_mut()),
                None => Node::Null,
            };
            node = node.with("then", then).with("else", otherwise);
            let then_list = node["then"].as_array().cloned().unwrap_or_default();
            let else_list = node["else"].as_array().cloned().unwrap_or_default();
            merge_banks(banks, vec![(then_banks, &then_list), (else_banks, &else_list)], key.as_ref());
        } else if kind == "while" {
            let cond = sound_expression(&node["cond"], found, banks, flow, &mut info);
            node = node.with("cond", cond);
            let mut body_banks = banks.clone();
            let body = named_sounds(node.list("body"), found, &mut body_banks, flow, info.as_deref_mut());
            node = node.with("body", body);
            let unchanged = banks.clone();
            merge_banks(banks, vec![(body_banks, &[]), (unchanged, &[])], None);
        } else if kind == "expr" && node["expr"].kind() == "assign" {
            let target = node["expr"]["target"].clone();
            let value = sound_expression(&node["expr"]["value"], found, banks, flow, &mut info);
            let expr = node["expr"].clone().with("value", value.clone());
            node = node.with("expr", expr);
            let source = if value.kind() == "var" { Some(value.text("name").to_string()) } else { None };
            let written = pointer_target(&target, flow).or_else(|| assigned_var(&target));
            match (&written, source.as_ref().and_then(|source| banks.vars.get(source).cloned())) {
                (Some(written), Some(bank)) => set_bank(&mut banks.vars, written, &bank, flow, &mut info),
                _ => {
                    if literal_value(&value, found).is_some() {
                        // a number written to a bank variable marks it empty (id = -1), not another
                        // bank
                        if let Some(written) = &written {
                            banks.vars.remove(written);
                            invalidate(&mut banks.vars, written);
                        }
                    } else {
                        forget_bank(&mut banks.vars, written.as_deref(), flow, &mut info);
                    }
                }
            }
        } else if kind == "native" || kind == "call" {
            node = sound_expression(&node, found, banks, flow, &mut info);
            if kind == "native" {
                let written = native_banks(&node, found, banks, flow);
                for (target, bank) in written.vars {
                    match bank {
                        None => forget_bank(&mut banks.vars, Some(&target), flow, &mut info),
                        Some(bank) => set_bank(&mut banks.vars, &target, &bank, flow, &mut info),
                    }
                }
                if let Some(port) = written.port {
                    banks.port = port;
                }
            }
        } else if kind == "expr" || kind == "return" {
            let key = if kind == "expr" { "expr" } else { "value" };
            if node[key].as_object().is_some() {
                let value = sound_expression(&node[key], found, banks, flow, &mut info);
                node = node.with(key, value);
            }
        }
        out.push(node);
    }
    Node::List(out)
}

const EXPRESSION_KEYS: [&str; 7] = ["args", "left", "right", "operand", "value", "target", "expr"];

/// Names the entries of the sound natives in an expression, and follows the banks the script's
/// functions it calls are passed and write through their pointer parameters. Inner calls first,
/// as they run first.
fn sound_expression(
    node: &Node,
    found: &Context,
    banks: &mut Banks,
    flow: Option<&Flow>,
    info: &mut Option<&mut Info>,
) -> Node {
    let map = match node {
        Node::List(items) => {
            return Node::List(items.iter().map(|item| sound_expression(item, found, banks, flow, info)).collect())
        }
        Node::Map(map) if !matches!(node.kind(), "if" | "while") => map,
        other => return other.clone(),
    };
    let mut rebuilt = crate::node::Map::with_capacity(map.len());
    for (key, value) in map.entries() {
        let value = if EXPRESSION_KEYS.contains(&key.as_ref()) {
            sound_expression(value, found, banks, flow, info)
        } else {
            value.clone()
        };
        rebuilt.insert(key.clone(), value);
    }
    let mut node = Node::Map(rebuilt);
    let kind = node.kind().to_string();
    if kind != "native" && kind != "call" {
        return node;
    }
    let args = node.list("args").to_vec();
    if let Some((bank, entry)) = sound_entry_args(node.text("name")) {
        if args.len() > entry {
            if let Some(kind) = assigned_var(&args[bank]).and_then(|name| plain(banks.vars.get(&name)).cloned()) {
                let mut args = args.clone();
                args[entry] = named_literal(&args[entry], &kind, found);
                node = node.with("args", args);
            }
        }
    }
    if kind == "call" {
        let name = node.text("name").to_string();
        if let Some(info) = info.as_deref_mut() {
            info.port_in.entry(name.clone()).or_default().insert(banks.port_passed());
        }
        // the call leaves the port as the function does, once the pass knows
        if let Some(left) = flow.and_then(|flow| flow.known.port_out.get(&name)) {
            banks.port = PortValue::after_call(Some(left), &banks.port);
        }
        for (position, arg) in args.iter().enumerate() {
            let key = (name.clone(), position);
            let out_bank = flow.and_then(|flow| flow.known.outs.get(&key));
            if !matches!(arg.kind(), "addr" | "int") && info.is_some() {
                // a number passed for a bank says there is none (-1), not another bank
                let passed = if arg.kind() == "var" {
                    plain(banks.vars.get(arg.text("name"))).cloned().unwrap_or_else(|| UNKNOWN_BANK.into())
                } else {
                    UNKNOWN_BANK.into()
                };
                if let Some(info) = info.as_deref_mut().filter(|_| literal_value(arg, found).is_none()) {
                    info.params.entry(key).or_default().insert(passed);
                }
            } else if arg.kind() == "addr" {
                match (out_bank, assigned_var(arg)) {
                    (Some(bank), Some(target)) => {
                        let bank = bank.clone();
                        set_bank(&mut banks.vars, &target, &bank, flow, info);
                    }
                    (Some(_), None) => {}
                    // the function may write anything through the pointer
                    (None, target) => {
                        if let Some(target) = target {
                            banks.vars.remove(&target);
                            invalidate(&mut banks.vars, &target);
                        }
                    }
                }
            }
        }
    }
    node
}

/// What a native call writes: variables, each with the bank it then holds (None for anything
/// else), and the event port when it loads there or empties it.
struct Written {
    vars: IndexMap<String, Option<String>>,
    port: Option<PortValue>,
}

/// What a native call writes.
fn native_banks(node: &Node, found: &Context, banks: &Banks, flow: Option<&Flow>) -> Written {
    let args = node.list("args");
    let mut written: IndexMap<String, Option<String>> = IndexMap::new();
    let mut port_written = None;
    let result = node.get("result").filter(|result| !result.is_null());
    for target in result.into_iter().chain(args.iter().filter(|arg| arg.kind() == "addr")) {
        if let Some(name) = assigned_var(target) {
            written.insert(name, None);
        }
    }
    let result = result.and_then(assigned_var);
    match node.text("name") {
        "get_system_sound_id" if result.is_some() => {
            written.insert(result.unwrap(), Some("system_se".into()));
        }
        "get_sound_id" if args.len() > 1 && assigned_var(&args[1]).is_some() => {
            let bank = if args[0].kind() == "var" {
                args[0]["name"].clone()
            } else {
                args[0].get("value").cloned().unwrap_or(Node::Null)
            };
            written.insert(assigned_var(&args[1]).unwrap(), scene_bank(&bank).map(str::to_string));
        }
        "load_sound" if args.len() >= 2 => {
            let loaded = loaded_bank(node, found);
            let last = &args[args.len() - 1];
            let mut target = result.clone().or_else(|| if last.kind() == "addr" { assigned_var(last) } else { None });
            if target.is_none()
                && last.kind() == "var"
                && flow.is_some_and(|flow| flow.pointers.contains_key(last.text("name")))
            {
                target = Some(last.text("name").to_string());
            }
            // the two-argument form, load_sound(set, &id), loads on the event port
            let has_port = args.len() > 2 || args[1].kind() != "addr";
            let port = if has_port {
                if args[0].kind() == "var" {
                    args[0]["name"].clone()
                } else {
                    args[0].get("value").cloned().unwrap_or(Node::Null)
                }
            } else {
                Node::from("SND_PORT_EVENT")
            };
            if let Some(target) = target {
                written.insert(target, loaded.clone());
            }
            let event_value = literal_value(&crate::node!({"kind": "var", "name": "SND_PORT_EVENT"}), found)
                .map_or(Node::Null, Node::Int);
            if port == "SND_PORT_EVENT" || port.same_value(&event_value) {
                port_written = Some(loaded.as_deref().map_or(PortValue::Unknown, PortValue::bank));
            }
        }
        "get_sound_port_bank" if result.is_some() => {
            let port = args.first().filter(|arg| arg.kind() == "var").map(|arg| arg.text("name"));
            if port.is_none() || port == Some("SND_PORT_EVENT") {
                // the dungeon engine plays entry 0 of the event port's bank as the battle music
                // (DngBattleBgm, dng_event.cpp); a function that keeps its volume handles that
                let bank = banks
                    .port
                    .only_bank()
                    .map(str::to_string)
                    .or_else(|| flow.is_some_and(|flow| flow.battle_music).then(|| EVENT_PORT_KIND.to_string()));
                written.insert(result.unwrap(), bank);
            }
        }
        "reset_sound_port" | "delete_sound_port"
            if args.first().is_some_and(|arg| arg.kind() == "var" && arg.text("name") == "SND_PORT_EVENT") =>
        {
            // the port holds something the pass cannot name, not nothing
            port_written = Some(PortValue::Unknown);
        }
        _ => {}
    }
    Written { vars: written, port: port_written }
}

fn single_kinds<K: Clone + std::hash::Hash + Eq>(found: &IndexMap<K, BTreeSet<String>>) -> HashMap<K, String> {
    found
        .iter()
        .filter(|(_, kinds)| kinds.len() == 1 && !kinds.contains(UNKNOWN_BANK))
        .map(|(key, kinds)| (key.clone(), kinds.iter().next().unwrap().clone()))
        .collect()
}

fn called_functions(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| called_functions(item, out)),
        Node::Map(map) => {
            if node.kind() == "call" {
                out.push(node.text("name").to_string());
            }
            map.values().for_each(|value| called_functions(value, out));
        }
        _ => {}
    }
}

/// The functions callers first (reverse postorder of the call graph), and for each whether all its
/// callers come before it, so the port it starts with is known when the pass reaches it.
fn callers_first<'a>(functions: &[&'a Node]) -> (Vec<&'a Node>, HashMap<String, bool>, HashSet<String>) {
    let position: HashMap<&str, usize> =
        functions.iter().enumerate().rev().map(|(index, function)| (function.text("name"), index)).collect();
    let callees: Vec<Vec<usize>> = functions
        .iter()
        .map(|function| {
            let mut called = Vec::new();
            called_functions(&function["body"], &mut called);
            called.iter().filter_map(|name| position.get(name.as_str()).copied()).collect()
        })
        .collect();
    let mut seen = vec![false; functions.len()];
    let mut postorder = Vec::with_capacity(functions.len());
    for root in 0..functions.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&callee) = callees[*node].get(*next) {
                *next += 1;
                if !seen[callee] {
                    seen[callee] = true;
                    stack.push((callee, 0));
                }
            } else {
                postorder.push(*node);
                stack.pop();
            }
        }
    }
    postorder.reverse();
    let order: HashMap<usize, usize> = postorder.iter().enumerate().map(|(at, &index)| (index, at)).collect();
    let mut complete: HashMap<String, bool> =
        functions.iter().map(|function| (function.text("name").to_string(), true)).collect();
    let mut called = HashSet::new();
    for (caller, targets) in callees.iter().enumerate() {
        for &callee in targets {
            called.insert(functions[callee].text("name").to_string());
            if order[&caller] >= order[&callee] {
                complete.insert(functions[callee].text("name").to_string(), false);
            }
        }
    }
    (postorder.into_iter().map(|index| functions[index]).collect(), complete, called)
}

/// The bank the events that change the event port leave there, when they agree. stays: what each
/// function leaves where it returns on the map (None when it never does).
fn resting_port(started: &HashSet<String>, stays: &HashMap<String, Option<PortValue>>) -> Option<String> {
    let left = PortValue::joined(started.iter().filter_map(|name| stays.get(name)?.as_ref()))?;
    match left.banks()?.into_iter().collect::<Vec<_>>()[..] {
        [bank] => Some(bank.to_string()),
        _ => None,
    }
}

fn settled(kinds: Option<&BTreeSet<String>>) -> Option<String> {
    let kinds = kinds?;
    (kinds.len() == 1 && !kinds.contains(UNKNOWN_BANK)).then(|| kinds.iter().next().unwrap().clone())
}

/// Natives that ask to leave the map when the event ends: the map's other events never run after.
const LEAVES_MAP: [&str; 6] =
    ["jump_to_map", "go_to_interior", "go_to_outside", "enter_interior", "load_event_script", "run_pending_map_jump"];

/// Whether statements call a native that loads on a sound port or leaves the map.
fn touches_port(node: &Node) -> bool {
    match node {
        Node::List(items) => items.iter().any(touches_port),
        Node::Map(map) => {
            (node.kind() == "native" && {
                let name = node.text("name");
                matches!(name, "load_sound" | "reset_sound_port" | "delete_sound_port") || LEAVES_MAP.contains(&name)
            }) || map.values().any(touches_port)
        }
        _ => false,
    }
}

/// The event port along a path: what it may hold, and what it may hold if the path stays on the map
/// (None once it asked to leave).
#[derive(Clone)]
struct PortPath {
    all: PortValue,
    stay: Option<PortValue>,
}

impl PortPath {
    fn merged<'a>(paths: impl IntoIterator<Item = &'a PortPath>) -> Option<PortPath> {
        let paths: Vec<&PortPath> = paths.into_iter().collect();
        let all = PortValue::joined(paths.iter().map(|path| &path.all))?;
        let stay = PortValue::joined(paths.iter().filter_map(|path| path.stay.as_ref()));
        Some(PortPath { all, stay })
    }
}

/// What a function leaves on the event port where it returns, and where it returns without having
/// asked to leave the map; None where it never does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PortOut {
    all: Option<PortValue>,
    stay: Option<PortValue>,
}

impl PortOut {
    fn add(&mut self, path: &PortPath) {
        self.all = PortValue::joined(self.all.iter().chain([&path.all]));
        if let Some(stay) = &path.stay {
            self.stay = PortValue::joined(self.stay.iter().chain([stay]));
        }
    }
}

/// Follows the event port through an expression: calls leave what the function leaves, and loads
/// put their bank there.
fn port_expression(
    node: &Node,
    found: &Context,
    path: &mut PortPath,
    outs: &HashMap<String, PortOut>,
    loaded: &mut BTreeSet<Option<String>>,
) {
    match node {
        Node::List(items) => items.iter().for_each(|item| port_expression(item, found, path, outs, loaded)),
        Node::Map(map) if !matches!(node.kind(), "if" | "while") => {
            for (key, value) in map.entries() {
                if EXPRESSION_KEYS.contains(&key.as_ref()) {
                    port_expression(value, found, path, outs, loaded);
                }
            }
            let name = node.text("name");
            match node.kind() {
                "call" => {
                    if let Some(out) = outs.get(name) {
                        path.all = PortValue::after_call(out.all.as_ref(), &path.all);
                        path.stay = match (&path.stay, &out.stay) {
                            (Some(stay), Some(left)) => Some(PortValue::after_call(Some(left), stay)),
                            _ => None,
                        };
                    }
                }
                "native" if matches!(name, "load_sound" | "reset_sound_port" | "delete_sound_port") => {
                    if let Some(port) = native_banks(node, found, &Banks::default(), None).port {
                        loaded.insert(port.only_bank().map(str::to_string));
                        if path.stay.is_some() {
                            path.stay = Some(port.clone());
                        }
                        path.all = port;
                    }
                }
                "native" if LEAVES_MAP.contains(&name) => path.stay = None,
                _ => {}
            }
        }
        _ => {}
    }
}

/// Follows the event port through statements, as named_sounds follows banks; the port where the
/// function returns goes into exits.
fn port_statements(
    nodes: &[Node],
    found: &Context,
    path: &mut PortPath,
    outs: &HashMap<String, PortOut>,
    exits: &mut PortOut,
    loaded: &mut BTreeSet<Option<String>>,
) {
    for node in nodes {
        match node.kind() {
            "if" => {
                port_expression(&node["cond"], found, path, outs, loaded);
                let then = node.list("then");
                let otherwise = node.list("else");
                let (mut then_path, mut else_path) = (path.clone(), path.clone());
                port_statements(then, found, &mut then_path, outs, exits, loaded);
                port_statements(otherwise, found, &mut else_path, outs, exits, loaded);
                let going_on = [(&then_path, then), (&else_path, otherwise)]
                    .into_iter()
                    .filter(|(_, statements)| statements.last().is_none_or(|last| !ENDS_PATH.contains(&last.kind())))
                    .map(|(going, _)| going);
                if let Some(merged) = PortPath::merged(going_on) {
                    *path = merged;
                }
            }
            "while" => {
                port_expression(&node["cond"], found, path, outs, loaded);
                let mut body_path = path.clone();
                port_statements(node.list("body"), found, &mut body_path, outs, exits, loaded);
                if let Some(merged) = PortPath::merged([&*path, &body_path]) {
                    *path = merged;
                }
            }
            "return" => {
                port_expression(&node["value"], found, path, outs, loaded);
                exits.add(path);
            }
            _ => port_expression(node, found, path, outs, loaded),
        }
    }
}

/// What each function leaves on the event port where it returns: the port as it found it, or the
/// banks it or the functions it calls load there. functions: callees first. Also every bank they
/// load there (None for one the pass cannot name).
fn port_summaries(functions: &[&Node], found: &Context) -> (HashMap<String, PortOut>, BTreeSet<Option<String>>) {
    let mut outs: HashMap<String, PortOut> = HashMap::new();
    let mut loaded = BTreeSet::new();
    for _ in 0..8 {
        let mut changed = false;
        for function in functions {
            let mut path = PortPath { all: PortValue::at_entry(), stay: Some(PortValue::at_entry()) };
            let mut exits = PortOut::default();
            let body = function.list("body");
            port_statements(body, found, &mut path, &outs, &mut exits, &mut loaded);
            if body.last().is_none_or(|last| last.kind() != "return") {
                exits.add(&path);
            }
            let name = function.text("name");
            if outs.get(name) != Some(&exits) {
                outs.insert(name.to_string(), exits);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (outs, loaded)
}

/// What the sound pass learns across a program's functions: the bank a global always holds, the
/// bank every call passes for a parameter, the bank a helper writes through a pointer parameter,
/// and the event port's bank where each function starts and what it leaves there.
///
/// entries: the functions the engine starts (the programs and main); they, and the functions
/// nothing here calls, start with the port's resting bank: the one bank the programs that change
/// the port leave there, or failing that the only bank the script loads there. A library (no
/// entries) takes the port its functions start with from the scripts that include it (seed), and
/// where they disagree its uncalled functions start with the library's own resting bank.
///
/// A few rounds settle it, as one fact can lead to another (a helper's out-parameter feeding a
/// global). Also what the last round saw.
pub fn program_sounds(
    functions: &[Node],
    found: &Context,
    seed: Option<&Seed>,
    entries: Option<&HashSet<String>>,
) -> (Known, Info) {
    let functions: Vec<&Node> = functions.iter().filter(|function| function.kind() == "function").collect();
    let (ordered, complete, called) = callers_first(&functions);
    let uncalled: HashSet<String> = functions
        .iter()
        .map(|function| function.text("name").to_string())
        .filter(|name| !called.contains(name))
        .collect();
    // only functions that load on the port or leave the map, and their callers, change the port
    let mut related: HashSet<&str> = functions
        .iter()
        .filter(|function| touches_port(&function["body"]))
        .map(|function| function.text("name"))
        .collect();
    let calls: Vec<Vec<String>> = if related.is_empty() {
        Vec::new()
    } else {
        functions
            .iter()
            .map(|function| {
                let mut called = Vec::new();
                called_functions(&function["body"], &mut called);
                called
            })
            .collect()
    };
    let mut grown = !related.is_empty();
    while grown {
        grown = false;
        for (function, called) in functions.iter().zip(&calls) {
            let name = function.text("name");
            if !related.contains(name) && called.iter().any(|callee| related.contains(callee.as_str())) {
                related.insert(name);
                grown = true;
            }
        }
    }
    let callees_first: Vec<&Node> =
        ordered.iter().rev().copied().filter(|function| related.contains(function.text("name"))).collect();
    let (port_out, loaded) = port_summaries(&callees_first, found);
    // the programs that set the port's resting bank: the script's, or a library's uncalled functions
    let programs: HashSet<String> = match entries {
        Some(entries) => functions
            .iter()
            .map(|function| function.text("name").to_string())
            .filter(|name| entries.contains(name))
            .collect(),
        None => uncalled.clone(),
    };
    // where the programs return on the map
    let stays: HashMap<String, Option<PortValue>> =
        port_out.iter().map(|(name, out)| (name.clone(), out.stay.clone())).collect();
    // failing that, the only bank the script loads there
    let only_loaded = match loaded.into_iter().collect::<Vec<_>>()[..] {
        [Some(ref bank)] => Some(bank.clone()),
        _ => None,
    };
    let port = resting_port(&programs, &stays).or(only_loaded);
    let port_out = port_out.into_iter().map(|(name, out)| (name, out.all.unwrap_or_default())).collect();
    let mut known = Known { port, port_out, ..Known::default() };
    // a library's own resting bank, for uncalled functions the scripts disagree on
    let fallback = |port_in: &mut HashMap<String, String>, port: &Option<String>| {
        if let (None, Some(port)) = (entries, port) {
            for name in &uncalled {
                port_in.entry(name.clone()).or_insert_with(|| port.clone());
            }
        }
    };
    let mut info = Info::default();
    for _ in 0..8 {
        let before = known.clone();
        info = Info::default();
        // what the scripts that include a library pass it counts as one more caller
        if let Some(seed) = seed {
            for (key, kind) in &seed.globals {
                info.globals.entry(key.clone()).or_default().insert(kind.clone());
            }
            for (key, kind) in &seed.params {
                info.params.entry(key.clone()).or_default().insert(kind.clone());
            }
            for (name, kind) in &seed.port_in {
                // where the scripts disagree, a library with a resting bank of its own follows it
                if kind != UNKNOWN_BANK || known.port.is_none() {
                    info.port_in.entry(name.clone()).or_default().insert(kind.clone());
                }
            }
        }
        if entries.is_some() {
            let resting = known.port.clone().unwrap_or_else(|| UNKNOWN_BANK.into());
            for name in programs.union(&uncalled) {
                info.port_in.entry(name.clone()).or_default().insert(resting.clone());
            }
        }
        for function in &ordered {
            let name = function.text("name");
            if complete.get(name) == Some(&true) {
                // every call to it was seen this round
                let library_resting = known.port.clone().filter(|_| entries.is_none() && uncalled.contains(name));
                match settled(info.port_in.get(name)).or(library_resting) {
                    Some(port) => known.port_in.insert(name.to_string(), port),
                    None => known.port_in.remove(name),
                };
            }
            let flow = sound_flow(function, &known);
            let mut banks = sound_banks(function, &known);
            named_sounds(function.list("body"), found, &mut banks, Some(&flow), Some(&mut info));
        }
        let mut port_in = single_kinds(&info.port_in);
        fallback(&mut port_in, &known.port);
        let learned = Known {
            globals: single_kinds(&info.globals),
            params: single_kinds(&info.params),
            outs: single_kinds(&info.outs),
            port: known.port.clone(),
            port_in,
            port_out: known.port_out.clone(),
        };
        known = learned;
        if known == before {
            break;
        }
    }
    (known, info)
}

/// What a script shows about the library code it includes: the banks it passes each parameter of
/// the hidden functions, and the bank each global they use holds (UNKNOWN_BANK when not one).
pub fn passed_sounds(hidden: &[Node], known: &Known, seen: &Info) -> Passed {
    let hidden_names: HashSet<&str> = hidden.iter().map(|function| function.text("name")).collect();
    let params = seen
        .params
        .iter()
        .filter(|(key, _)| hidden_names.contains(key.0.as_str()))
        .map(|(key, kinds)| (key.clone(), kinds.clone()))
        .collect();
    let mut used: Vec<String> = Vec::new();
    for function in hidden {
        let own: HashSet<&str> = function
            .list("params")
            .iter()
            .chain(function.list("locals"))
            .map(|variable| variable.text("name"))
            .collect();
        for name in crate::names::variables(&function["body"]) {
            if !own.contains(name.as_str()) && !used.contains(&name) {
                used.push(name);
            }
        }
    }
    let globals = used
        .into_iter()
        .filter(|name| seen.globals.contains_key(name) || known.globals.contains_key(name))
        .map(|name| {
            let bank = known.globals.get(&name).cloned().unwrap_or_else(|| UNKNOWN_BANK.into());
            (name, BTreeSet::from([bank]))
        })
        .collect();
    let port_in = seen
        .port_in
        .iter()
        .filter(|(name, _)| hidden_names.contains(name.as_str()))
        .map(|(name, kinds)| (name.clone(), kinds.clone()))
        .collect();
    Passed { params, globals, port_in }
}

/// The seed for a library's sound pass from what each script that includes it passes (passed per
/// script): a parameter, global or starting event port every script agrees on keeps that bank,
/// others are unknown.
pub fn library_sounds(passed: &[Passed]) -> Seed {
    let mut params: IndexMap<ParamKey, BTreeSet<String>> = IndexMap::new();
    let mut globals: IndexMap<String, BTreeSet<String>> = IndexMap::new();
    let mut port_in: IndexMap<String, BTreeSet<String>> = IndexMap::new();
    for found in passed {
        for (key, kinds) in &found.params {
            params.entry(key.clone()).or_default().extend(kinds.iter().cloned());
        }
        for (key, kinds) in &found.globals {
            globals.entry(key.clone()).or_default().extend(kinds.iter().cloned());
        }
        for (name, kinds) in &found.port_in {
            port_in.entry(name.clone()).or_default().extend(kinds.iter().cloned());
        }
    }
    let settle = |kinds: &BTreeSet<String>| {
        if kinds.len() == 1 {
            kinds.iter().next().unwrap().clone()
        } else {
            UNKNOWN_BANK.to_string()
        }
    };
    Seed {
        params: params.iter().map(|(key, kinds)| (key.clone(), settle(kinds))).collect(),
        globals: globals.iter().map(|(key, kinds)| (key.clone(), settle(kinds))).collect(),
        port_in: port_in.iter().map(|(name, kinds)| (name.clone(), settle(kinds))).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn banks(held: &[&str], at_entry: bool) -> PortValue {
        let mut set: BTreeSet<Held> = held.iter().map(|bank| Held::Bank(bank.to_string())).collect();
        if at_entry {
            set.insert(Held::AtEntry);
        }
        PortValue::OneOf(set)
    }

    #[test]
    fn port_values_join_to_what_either_path_left() {
        let a = PortValue::bank("se_a");
        let b = PortValue::bank("se_b");
        assert_eq!(a.join(&b), banks(&["se_a", "se_b"], false));
        assert_eq!(a.join(&a), a);
        assert_eq!(a.join(&PortValue::at_entry()), banks(&["se_a"], true));
        // unknown absorbs everything
        assert_eq!(a.join(&PortValue::Unknown), PortValue::Unknown);
        assert_eq!(PortValue::Unknown.join(&PortValue::at_entry()), PortValue::Unknown);
        assert_eq!(PortValue::joined([&a, &b, &a]), Some(banks(&["se_a", "se_b"], false)));
        assert_eq!(PortValue::joined([]), None);
    }

    #[test]
    fn only_one_bank_is_a_bank_the_port_holds() {
        assert_eq!(PortValue::bank("se_a").only_bank(), Some("se_a"));
        assert_eq!(banks(&["se_a", "se_b"], false).only_bank(), None);
        assert_eq!(banks(&["se_a"], true).only_bank(), None);
        assert_eq!(PortValue::at_entry().only_bank(), None);
        assert_eq!(PortValue::Unknown.only_bank(), None);
        assert_eq!(Banks::default().port_passed(), UNKNOWN_BANK);
    }

    #[test]
    fn a_call_leaves_the_port_as_the_function_does() {
        let before = PortValue::bank("se_caller");
        // a function that leaves the port as it found it
        assert_eq!(PortValue::after_call(Some(&PortValue::at_entry()), &before), before);
        // one that loads a bank on some paths
        assert_eq!(PortValue::after_call(Some(&banks(&["se_x"], true)), &before), banks(&["se_caller", "se_x"], false));
        // one that always loads
        assert_eq!(PortValue::after_call(Some(&PortValue::bank("se_x")), &before), PortValue::bank("se_x"));
        // what the caller held is unknown, and the function may leave it
        assert_eq!(PortValue::after_call(Some(&banks(&["se_x"], true)), &PortValue::Unknown), PortValue::Unknown);
        assert_eq!(PortValue::after_call(Some(&PortValue::Unknown), &before), PortValue::Unknown);
        // a function the pass saw return nowhere
        assert_eq!(PortValue::after_call(None, &before), PortValue::Unknown);
    }

    #[test]
    fn the_resting_port_is_the_one_bank_the_programs_leave_on_the_map() {
        let started: HashSet<String> = ["a", "b", "c"].iter().map(|name| name.to_string()).collect();
        let stays = |values: &[(&str, Option<PortValue>)]| -> HashMap<String, Option<PortValue>> {
            values.iter().map(|(name, value)| (name.to_string(), value.clone())).collect()
        };
        // programs that leave the port as they found it do not count
        let agreed = stays(&[("a", Some(banks(&["se_rest"], true))), ("b", Some(PortValue::at_entry())), ("c", None)]);
        assert_eq!(resting_port(&started, &agreed), Some("se_rest".to_string()));
        let differing = stays(&[("a", Some(PortValue::bank("se_rest"))), ("b", Some(PortValue::bank("se_other")))]);
        assert_eq!(resting_port(&started, &differing), None);
        let unknown = stays(&[("a", Some(PortValue::bank("se_rest"))), ("b", Some(PortValue::Unknown))]);
        assert_eq!(resting_port(&started, &unknown), None);
        assert_eq!(resting_port(&started, &stays(&[])), None);
    }

    fn var(name: &str) -> Node {
        crate::node!({"kind": "var", "name": name})
    }

    fn load_event(path: &str) -> Node {
        crate::node!({"kind": "native", "name": "load_sound", "args": [var("SND_PORT_EVENT"), {"kind": "string", "text": path}]})
    }

    /// The event port after statements, from the port as a function found it.
    fn port_after(statements: &[Node]) -> PortValue {
        let mut banks = Banks { port: PortValue::at_entry(), ..Banks::default() };
        named_sounds(statements, &Context::default(), &mut banks, None, None);
        banks.port
    }

    #[test]
    fn the_event_port_follows_loads_resets_and_branches() {
        let load_a = load_event("snd2/ev/ev_a.snd");
        let load_b = load_event("snd2/ev/ev_b.snd");
        assert_eq!(port_after(&[]), PortValue::at_entry());
        assert_eq!(port_after(std::slice::from_ref(&load_a)), PortValue::bank("se_ev_a"));
        let reset = crate::node!({"kind": "native", "name": "reset_sound_port", "args": [var("SND_PORT_EVENT")]});
        assert_eq!(port_after(&[load_a.clone(), reset]), PortValue::Unknown);
        // after an if, what either path left; a path that returns does not go on
        let branch = |otherwise: Node| crate::node!({"kind": "if", "cond": var("x"), "then": [load_a.clone()], "else": otherwise});
        assert_eq!(port_after(&[branch(Node::Null)]), banks(&["se_ev_a"], true));
        assert_eq!(port_after(&[branch(crate::node!([load_b.clone()]))]), banks(&["se_ev_a", "se_ev_b"], false));
        let returning = crate::node!([load_b.clone(), {"kind": "return", "value": null}]);
        assert_eq!(port_after(&[branch(returning)]), PortValue::bank("se_ev_a"));
        // a loop may run or not
        let looped = crate::node!({"kind": "while", "cond": var("x"), "body": [load_b.clone()]});
        assert_eq!(port_after(&[load_a, looped]), banks(&["se_ev_a", "se_ev_b"], false));
    }

    #[test]
    fn what_a_function_leaves_on_the_port_reaches_its_callers() {
        let function = |name: &str, body: Node| crate::node!({"kind": "function", "name": name, "params": [], "locals": [], "body": body});
        let loader = function("load_music", crate::node!([load_event("snd2/ev/ev_a.snd")]));
        let passer = function(
            "pass",
            crate::node!([{"kind": "expr", "expr": {"kind": "call", "name": "load_music", "args": []}}]),
        );
        let quiet = function("quiet", crate::node!([]));
        let found = Context::default();
        let functions = [loader, passer, quiet];
        let (outs, loaded) = port_summaries(&functions.iter().collect::<Vec<_>>(), &found);
        assert_eq!(outs["load_music"].all, Some(PortValue::bank("se_ev_a")));
        assert_eq!(outs["pass"].all, Some(PortValue::bank("se_ev_a")));
        assert_eq!(outs["quiet"].all, Some(PortValue::at_entry()));
        assert_eq!(loaded, BTreeSet::from([Some("se_ev_a".to_string())]));
        // with every function an entry, the only bank loaded is where the port rests
        let entries: HashSet<String> = ["quiet".to_string()].into();
        let (known, _) = program_sounds(&functions, &found, None, Some(&entries));
        assert_eq!(known.port.as_deref(), Some("se_ev_a"));
        assert_eq!(known.port_out["pass"], PortValue::bank("se_ev_a"));
    }
}
