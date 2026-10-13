//! The sound pass of presenting a script: names the entry number of play_se(id, n) and its kin by
//! the bank the id was read from (the system bank, a scene bank, a bank load_sound loaded, the
//! event port's latest bank), following ids through variables, globals, parameters and helpers'
//! out-parameters across a program's functions.

use std::collections::{BTreeSet, HashMap, HashSet};

use indexmap::IndexMap;

use crate::node::Node;
use crate::sdk::{named_literal, Context};
use crate::source::truthy;

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
        || matches!(name, "load_sound" | "get_sound_id" | "get_system_sound_id" | "get_sound_port_bank")
}

const EVENT_PORT: &str = "event port";
/// What the pass records for a variable that is written, or a parameter passed, something it cannot
/// follow; a variable seen with it is never settled.
pub const UNKNOWN_BANK: &str = "?";

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

fn list<'a>(node: &'a Node, key: &str) -> &'a [Node] {
    node[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

fn text<'a>(node: &'a Node, key: &str) -> &'a str {
    node[key].as_str().unwrap_or("")
}

fn assigned_var(node: &Node) -> Option<String> {
    match node.kind() {
        "var" => Some(text(node, "name").to_string()),
        "addr" if node["target"].kind() == "var" => Some(text(&node["target"], "name").to_string()),
        _ => None,
    }
}

fn bank_kind(stem: &str) -> String {
    format!("se_{}", stem.to_lowercase())
}

/// An int literal's value, or an int constant's.
fn literal_value(node: &Node, found: &Context) -> Option<i64> {
    match node.kind() {
        "int" => node["value"].as_i64(),
        "var" => {
            let value = found.constants.get(text(node, "name"))?;
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
    let args = list(node, "args");
    let source = if args.len() > 2 || args[1].kind() != "addr" { &args[1] } else { &args[0] };
    if source.kind() == "string" {
        let path = text(source, "text").replace('\\', "/");
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
    pub port: Option<String>,
}

/// What one round of the pass sees: every bank written to globals, passed for parameters and
/// written through out-parameters, and loaded on the event port.
#[derive(Clone, Debug, Default)]
pub struct Info {
    pub globals: IndexMap<String, BTreeSet<String>>,
    pub params: IndexMap<ParamKey, BTreeSet<String>>,
    pub outs: IndexMap<ParamKey, BTreeSet<String>>,
    pub port: BTreeSet<String>,
}

/// What the scripts that include a library pass it: a bank for each parameter and global they
/// agree on (UNKNOWN_BANK where they do not).
#[derive(Clone, Debug, Default)]
pub struct Seed {
    pub params: IndexMap<ParamKey, String>,
    pub globals: IndexMap<String, String>,
}

/// What a script shows about the library code it includes: the banks it passes each parameter of
/// the hidden functions, and the bank each global they use holds.
#[derive(Clone, Debug, Default)]
pub struct Passed {
    pub params: IndexMap<ParamKey, BTreeSet<String>>,
    pub globals: IndexMap<String, BTreeSet<String>>,
}

/// The sound pass's state across scripts: the seed for a library, where to put what a script
/// passes its libraries, and included functions prepared once per key.
#[derive(Default)]
pub struct Sounds {
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
}

pub fn sound_flow<'a>(function: &Node, known: &'a Known) -> Flow<'a> {
    let locals = list(function, "params")
        .iter()
        .chain(list(function, "locals"))
        .map(|variable| text(variable, "name").to_string())
        .collect();
    let pointers = list(function, "params")
        .iter()
        .enumerate()
        .filter(|(_, param)| truthy(&param["array"]))
        .map(|(position, param)| (text(param, "name").to_string(), position))
        .collect();
    Flow { function: text(function, "name").to_string(), locals, pointers, known }
}

/// The banks a function starts with: its globals and parameters the program pass settled, and the
/// event port's.
pub fn sound_banks(function: &Node, known: &Known) -> HashMap<String, String> {
    let locals: HashSet<&str> = list(function, "params")
        .iter()
        .chain(list(function, "locals"))
        .map(|variable| text(variable, "name"))
        .collect();
    let mut banks: HashMap<String, String> = known
        .globals
        .iter()
        .filter(|(name, _)| !locals.contains(name.as_str()))
        .map(|(name, kind)| (name.clone(), kind.clone()))
        .collect();
    let name = text(function, "name");
    for (position, param) in list(function, "params").iter().enumerate() {
        if let Some(kind) = known.params.get(&(name.to_string(), position)) {
            banks.insert(text(param, "name").to_string(), kind.clone());
        }
    }
    if let Some(port) = &known.port {
        banks.insert(EVENT_PORT.into(), port.clone());
    }
    banks
}

fn record_bank(name: &str, kind: &str, flow: Option<&Flow>, info: &mut Option<&mut Info>) {
    let (Some(flow), Some(info)) = (flow, info.as_deref_mut()) else {
        return;
    };
    if name == EVENT_PORT {
        info.port.insert(kind.to_string());
    } else if let Some(&position) = flow.pointers.get(name) {
        info.outs.entry((flow.function.clone(), position)).or_default().insert(kind.to_string());
    } else if !flow.locals.contains(name) {
        info.globals.entry(name.to_string()).or_default().insert(kind.to_string());
    }
}

fn set_bank(
    banks: &mut HashMap<String, String>,
    name: &str,
    kind: &str,
    flow: Option<&Flow>,
    info: &mut Option<&mut Info>,
) {
    banks.insert(name.to_string(), kind.to_string());
    record_bank(name, kind, flow, info);
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
    record_bank(name, UNKNOWN_BANK, flow, info);
}

/// The pointer parameter p an assignment writes through (p[0] = ... or *p = ...).
fn pointer_target(node: &Node, flow: Option<&Flow>) -> Option<String> {
    let flow = flow?;
    if node.kind() == "index" && flow.pointers.contains_key(text(node, "name")) {
        return Some(text(node, "name").to_string());
    }
    if node.kind() == "deref"
        && node["operand"].kind() == "var"
        && flow.pointers.contains_key(text(&node["operand"], "name"))
    {
        return Some(text(&node["operand"], "name").to_string());
    }
    None
}

const ENDS_PATH: [&str; 3] = ["return", "break", "continue"];

/// Sets banks to what the paths that go on agree on: a variable one path forgets, or two paths give
/// different banks, is forgotten; one only some paths set keeps that bank (on the other paths it was
/// never a bank id). A path whose statements end in a return, break or continue does not go on.
fn merge_banks(banks: &mut HashMap<String, String>, paths: Vec<(HashMap<String, String>, &[Node])>) {
    let going_on: Vec<HashMap<String, String>> = paths
        .into_iter()
        .filter(|(_, statements)| statements.last().is_none_or(|last| !ENDS_PATH.contains(&last.kind())))
        .map(|(path_banks, _)| path_banks)
        .collect();
    if going_on.is_empty() {
        return;
    }
    let before = std::mem::take(banks);
    let mut all: HashSet<&String> = before.keys().collect();
    for path_banks in &going_on {
        all.extend(path_banks.keys());
    }
    for name in all {
        let kinds: HashSet<Option<&String>> = going_on
            .iter()
            .filter(|path_banks| path_banks.contains_key(name) || before.contains_key(name))
            .map(|path_banks| path_banks.get(name))
            .collect();
        if kinds.len() == 1 {
            if let Some(Some(kind)) = kinds.into_iter().next() {
                banks.insert(name.clone(), kind.clone());
            }
        }
    }
}

/// Names the entries of the sound natives in a function's statements and follows the banks, as
/// the Python's named_sounds.
pub fn named_sounds(
    nodes: &[Node],
    found: &Context,
    banks: &mut HashMap<String, String>,
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
            node = node.with("cond", cond);
            let (mut then_banks, mut else_banks) = (banks.clone(), banks.clone());
            let then = named_sounds(list(&node, "then"), found, &mut then_banks, flow, info.as_deref_mut());
            let otherwise = match node["else"].as_array() {
                Some(items) => named_sounds(items, found, &mut else_banks, flow, info.as_deref_mut()),
                None => Node::Null,
            };
            node = node.with("then", then).with("else", otherwise);
            let then_list = node["then"].as_array().cloned().unwrap_or_default();
            let else_list = node["else"].as_array().cloned().unwrap_or_default();
            merge_banks(banks, vec![(then_banks, &then_list), (else_banks, &else_list)]);
        } else if kind == "while" {
            let cond = sound_expression(&node["cond"], found, banks, flow, &mut info);
            node = node.with("cond", cond);
            let mut body_banks = banks.clone();
            let body = named_sounds(list(&node, "body"), found, &mut body_banks, flow, info.as_deref_mut());
            node = node.with("body", body);
            let unchanged = banks.clone();
            merge_banks(banks, vec![(body_banks, &[]), (unchanged, &[])]);
        } else if kind == "expr" && node["expr"].kind() == "assign" {
            let target = node["expr"]["target"].clone();
            let value = sound_expression(&node["expr"]["value"], found, banks, flow, &mut info);
            let expr = node["expr"].clone().with("value", value.clone());
            node = node.with("expr", expr);
            let source = if value.kind() == "var" { Some(text(&value, "name").to_string()) } else { None };
            let written = pointer_target(&target, flow).or_else(|| assigned_var(&target));
            match (&written, source.as_ref().and_then(|source| banks.get(source).cloned())) {
                (Some(written), Some(bank)) => set_bank(banks, written, &bank, flow, &mut info),
                _ => {
                    if literal_value(&value, found).is_some() {
                        // a number written to a bank variable marks it empty (id = -1), not another
                        // bank
                        if let Some(written) = &written {
                            banks.remove(written);
                        }
                    } else {
                        forget_bank(banks, written.as_deref(), flow, &mut info);
                    }
                }
            }
        } else if kind == "native" || kind == "call" {
            node = sound_expression(&node, found, banks, flow, &mut info);
            if kind == "native" {
                for (target, bank) in native_banks(&node, found, banks, flow) {
                    match bank {
                        None => forget_bank(banks, Some(&target), flow, &mut info),
                        Some(bank) => set_bank(banks, &target, &bank, flow, &mut info),
                    }
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
    banks: &mut HashMap<String, String>,
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
    let args = list(&node, "args").to_vec();
    if let Some((bank, entry)) = sound_entry_args(text(&node, "name")) {
        if args.len() > entry {
            if let Some(kind) = assigned_var(&args[bank]).and_then(|name| banks.get(&name).cloned()) {
                let mut args = args.clone();
                args[entry] = named_literal(&args[entry], &kind, found);
                node = node.with("args", args);
            }
        }
    }
    if kind == "call" {
        let name = text(&node, "name").to_string();
        for (position, arg) in args.iter().enumerate() {
            let key = (name.clone(), position);
            let out_bank = flow.and_then(|flow| flow.known.outs.get(&key));
            if !matches!(arg.kind(), "addr" | "int") && info.is_some() {
                // a number passed for a bank says there is none (-1), not another bank
                let passed = if arg.kind() == "var" {
                    banks.get(text(arg, "name")).cloned().unwrap_or_else(|| UNKNOWN_BANK.into())
                } else {
                    UNKNOWN_BANK.into()
                };
                if let Some(info) = info.as_deref_mut() {
                    info.params.entry(key).or_default().insert(passed);
                }
            } else if arg.kind() == "addr" {
                match (out_bank, assigned_var(arg)) {
                    (Some(bank), Some(target)) => {
                        let bank = bank.clone();
                        set_bank(banks, &target, &bank, flow, info);
                    }
                    (Some(_), None) => {}
                    // the function may write anything through the pointer
                    (None, target) => {
                        if let Some(target) = target {
                            banks.remove(&target);
                        }
                    }
                }
            }
        }
    }
    node
}

/// The variables a native call writes and the bank each then holds (None for anything else).
fn native_banks(
    node: &Node,
    found: &Context,
    banks: &HashMap<String, String>,
    flow: Option<&Flow>,
) -> IndexMap<String, Option<String>> {
    let args = list(node, "args");
    let mut written: IndexMap<String, Option<String>> = IndexMap::new();
    let result = node.get("result").filter(|result| !result.is_null());
    for target in result.into_iter().chain(args.iter().filter(|arg| arg.kind() == "addr")) {
        if let Some(name) = assigned_var(target) {
            written.insert(name, None);
        }
    }
    let result = result.and_then(assigned_var);
    match text(node, "name") {
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
                && flow.is_some_and(|flow| flow.pointers.contains_key(text(last, "name")))
            {
                target = Some(text(last, "name").to_string());
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
                .map(Node::Int)
                .unwrap_or(Node::Null);
            if port == "SND_PORT_EVENT" || port.py_eq(&event_value) {
                written.insert(EVENT_PORT.into(), loaded);
            }
        }
        "get_sound_port_bank" if result.is_some() && banks.contains_key(EVENT_PORT) => {
            let port = args.first().filter(|arg| arg.kind() == "var").map(|arg| text(arg, "name"));
            if port.is_none() || port == Some("SND_PORT_EVENT") {
                written.insert(result.unwrap(), banks.get(EVENT_PORT).cloned());
            }
        }
        _ => {}
    }
    written
}

fn single_kinds<K: Clone + std::hash::Hash + Eq>(found: &IndexMap<K, BTreeSet<String>>) -> HashMap<K, String> {
    found
        .iter()
        .filter(|(_, kinds)| kinds.len() == 1 && !kinds.contains(UNKNOWN_BANK))
        .map(|(key, kinds)| (key.clone(), kinds.iter().next().unwrap().clone()))
        .collect()
}

/// What the sound pass learns across a program's functions: the bank a global always holds, the
/// bank every call passes for a parameter, the bank a helper writes through a pointer parameter,
/// and the event port's bank when the program loads only one there. A few rounds settle it, as one
/// fact can lead to another (a helper's out-parameter feeding a global). Also what the last round
/// saw.
pub fn program_sounds(functions: &[Node], found: &Context, seed: Option<&Seed>) -> (Known, Info) {
    let mut known = Known::default();
    let mut info = Info::default();
    for _ in 0..4 {
        info = Info::default();
        // what the scripts that include a library pass it counts as one more caller
        if let Some(seed) = seed {
            for (key, kind) in &seed.globals {
                info.globals.entry(key.clone()).or_default().insert(kind.clone());
            }
            for (key, kind) in &seed.params {
                info.params.entry(key.clone()).or_default().insert(kind.clone());
            }
        }
        for function in functions.iter().filter(|function| function.kind() == "function") {
            let flow = sound_flow(function, &known);
            let mut banks = sound_banks(function, &known);
            named_sounds(list(function, "body"), found, &mut banks, Some(&flow), Some(&mut info));
        }
        let port = single_kinds(&IndexMap::from([("port".to_string(), info.port.clone())])).remove("port");
        let learned = Known {
            globals: single_kinds(&info.globals),
            params: single_kinds(&info.params),
            outs: single_kinds(&info.outs),
            port,
        };
        if learned == known {
            break;
        }
        known = learned;
    }
    (known, info)
}

/// What a script shows about the library code it includes: the banks it passes each parameter of
/// the hidden functions, and the bank each global they use holds (UNKNOWN_BANK when not one).
pub fn passed_sounds(hidden: &[Node], known: &Known, seen: &Info) -> Passed {
    let hidden_names: HashSet<&str> = hidden.iter().map(|function| text(function, "name")).collect();
    let params = seen
        .params
        .iter()
        .filter(|(key, _)| hidden_names.contains(key.0.as_str()))
        .map(|(key, kinds)| (key.clone(), kinds.clone()))
        .collect();
    let mut used: Vec<String> = Vec::new();
    for function in hidden {
        let own: HashSet<&str> = list(function, "params")
            .iter()
            .chain(list(function, "locals"))
            .map(|variable| text(variable, "name"))
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
    Passed { params, globals }
}

/// The seed for a library's sound pass from what each script that includes it passes (passed per
/// script): a parameter or global every script agrees on keeps that bank, others are unknown.
pub fn library_sounds(passed: &[Passed]) -> Seed {
    let mut params: IndexMap<ParamKey, BTreeSet<String>> = IndexMap::new();
    let mut globals: IndexMap<String, BTreeSet<String>> = IndexMap::new();
    for found in passed {
        for (key, kinds) in &found.params {
            params.entry(key.clone()).or_default().extend(kinds.iter().cloned());
        }
        for (key, kinds) in &found.globals {
            globals.entry(key.clone()).or_default().extend(kinds.iter().cloned());
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
    }
}
