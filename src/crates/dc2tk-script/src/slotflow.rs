//! Character slots named where a load reaches them: after load_chara(..., "e438_paw.chr", ..., 8)
//! slot 8 holds that character until the function loads it again, deletes it or calls a function
//! that may, so the slot's uses print as the character's constant for the folder
//! (Context::slot_packs, from slots::loaded_names). Follows the slots through a function's
//! branches: after an if, a slot keeps the character every path that goes on agrees on.

use std::collections::{HashMap, HashSet};

use crate::node::Node;
use crate::sdk::Context;

/// A native's SDK name, from either name a call carries.
fn sdk_name<'a>(node: &'a Node, found: &'a Context) -> &'a str {
    let name = node.text("name");
    match found.by_original.get(name) {
        Some(declared) => declared["name"].as_str().unwrap_or(name),
        None => name,
    }
}

/// Natives that put a character into a slot: (pack position, slot position).
fn loads(name: &str) -> Option<(usize, usize)> {
    matches!(name, "load_chara" | "load_chara_motion").then_some((1, 3))
}

/// Natives that change what a slot holds other than by a pack load: delete_chara(slot),
/// copy_chara(stack, src, dst), load_npc_chara(set, stack, npc, slot), and the villager loaders.
fn changes(name: &str) -> Option<Option<usize>> {
    Some(match name {
        "delete_chara" => Some(0),
        "copy_chara" => Some(2),
        "load_npc_chara" => Some(3),
        "register_villager_here" => None,
        _ => return None,
    })
}

/// What a native or a helper call does to one slot: the slot (None when the pass cannot tell
/// which) and the pack a load puts there (None when it deletes it or the pack is not a literal).
struct Effect {
    slot: Option<i64>,
    pack: Option<String>,
}

/// The natives in a helper's body.
fn natives_in<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    match node {
        Node::List(items) => items.iter().for_each(|item| natives_in(item, out)),
        Node::Map(map) => {
            if node.kind() == "native" {
                out.push(node);
            }
            map.values().for_each(|value| natives_in(value, out));
        }
        _ => {}
    }
}

/// What a native call, or a call of a helper whose body makes such calls, does to the slots;
/// None when it changes none.
fn effects(node: &Node, found: &Context) -> Option<Vec<Effect>> {
    let native_effect = |name: &str, args: &[Node]| -> Option<Effect> {
        if let Some((pack_at, slot_at)) = loads(name) {
            let pack =
                args.get(pack_at).filter(|pack| pack.kind() == "string").map(|pack| pack.text("text").to_string());
            return Some(Effect { slot: args.get(slot_at).and_then(|slot| slot_value(slot, found)), pack });
        }
        let position = changes(name)?;
        Some(Effect {
            slot: position.and_then(|position| args.get(position)).and_then(|slot| slot_value(slot, found)),
            pack: None,
        })
    };
    match node.kind() {
        "native" => native_effect(sdk_name(node, found), node.list("args")).map(|effect| vec![effect]),
        "call" => {
            let inline = found.inlines.get(node.text("name"))?;
            let params: Vec<&str> = inline.list("params").iter().map(|param| param.text("name")).collect();
            let args = node.list("args");
            let mut natives = Vec::new();
            natives_in(&inline["body"], &mut natives);
            let found_effects: Vec<Effect> = natives
                .into_iter()
                .filter_map(|native| {
                    // the helper's parameters stand for the call's arguments
                    let bound: Vec<Node> = native
                        .list("args")
                        .iter()
                        .map(|arg| {
                            let at = (arg.kind() == "var")
                                .then(|| params.iter().position(|param| *param == arg.text("name")));
                            match at.flatten().and_then(|at| args.get(at)) {
                                Some(passed) => passed.clone(),
                                None => arg.clone(),
                            }
                        })
                        .collect();
                    native_effect(sdk_name(native, found), &bound)
                })
                .collect();
            (!found_effects.is_empty()).then_some(found_effects)
        }
        _ => None,
    }
}

/// What a function does to the slots, with the functions it calls: the literal slots it changes,
/// or None for any.
type Touched = Option<HashSet<i64>>;

fn touched_by(nodes: &Node, found: &Context, out: &mut Touched, calls: &mut Vec<String>) {
    match nodes {
        Node::List(items) => items.iter().for_each(|item| touched_by(item, found, out, calls)),
        Node::Map(map) => {
            if let Some(found_effects) = effects(nodes, found) {
                for effect in found_effects {
                    match (effect.slot, out.as_mut()) {
                        (Some(value), Some(set)) => {
                            set.insert(value);
                        }
                        (None, _) => *out = None,
                        _ => {}
                    }
                }
            } else if nodes.kind() == "call" && !found.inlines.contains_key(nodes.text("name")) {
                calls.push(nodes.text("name").to_string());
            }
            map.values().for_each(|value| touched_by(value, found, out, calls));
        }
        _ => {}
    }
}

/// A slot argument's number: a literal, a constant or a slot name.
fn slot_value(node: &Node, found: &Context) -> Option<i64> {
    match node.kind() {
        "int" => node["value"].as_i64(),
        "var" => {
            let name = node.text("name");
            found.constants.get(name).and_then(|value| value["value"].as_i64()).or_else(|| {
                found.enums.iter().filter(|(kind, _)| kind.starts_with("chara_slot")).find_map(|(_, members)| {
                    members.iter().find(|(_, member)| *member == name).map(|(value, _)| *value)
                })
            })
        }
        _ => None,
    }
}

/// What each function may change, following calls (a call to a function not here may change
/// any slot).
fn summaries(functions: &[&Node], found: &Context) -> HashMap<String, Touched> {
    let mut own: HashMap<String, (Touched, Vec<String>)> = HashMap::new();
    for function in functions {
        let mut touched = Some(HashSet::new());
        let mut calls = Vec::new();
        touched_by(&function["body"], found, &mut touched, &mut calls);
        own.insert(function.text("name").to_string(), (touched, calls));
    }
    let mut out: HashMap<String, Touched> =
        own.iter().map(|(name, (touched, _))| (name.clone(), touched.clone())).collect();
    let mut changed = true;
    while changed {
        changed = false;
        for (name, (_, calls)) in &own {
            let mut touched = out[name].clone();
            for callee in calls {
                match (out.get(callee).cloned().unwrap_or(None), touched.as_mut()) {
                    (Some(theirs), Some(mine)) => mine.extend(theirs),
                    (None, Some(_)) => touched = None,
                    _ => {}
                }
            }
            if touched != out[name] {
                out.insert(name.clone(), touched);
                changed = true;
            }
        }
    }
    out
}

/// Slot → the pack it holds, along a path.
type Slots = HashMap<i64, String>;

const ENDS_PATH: [&str; 3] = ["return", "break", "continue"];

fn merged(before: &Slots, paths: &[(Slots, &[Node])]) -> Slots {
    let going_on: Vec<&Slots> = paths
        .iter()
        .filter(|(_, statements)| statements.last().is_none_or(|last| !ENDS_PATH.contains(&last.kind())))
        .map(|(slots, _)| slots)
        .collect();
    if going_on.is_empty() {
        return before.clone();
    }
    going_on[0]
        .iter()
        .filter(|(slot, pack)| going_on[1..].iter().all(|other| other.get(slot) == Some(pack)))
        .map(|(slot, pack)| (*slot, pack.clone()))
        .collect()
}

struct Pass<'a> {
    found: &'a Context,
    touched: &'a HashMap<String, Touched>,
}

impl Pass<'_> {
    fn statements(&self, nodes: &[Node], slots: &mut Slots) -> Vec<Node> {
        nodes
            .iter()
            .map(|node| match node.kind() {
                "if" => {
                    let cond = self.expression(&node["cond"], slots);
                    let (mut then_slots, mut else_slots) = (slots.clone(), slots.clone());
                    let then = self.statements(node.list("then"), &mut then_slots);
                    let otherwise = node["else"].as_array().map(|items| self.statements(items, &mut else_slots));
                    *slots =
                        merged(slots, &[(then_slots, &then[..]), (else_slots, otherwise.as_deref().unwrap_or(&[]))]);
                    node.clone()
                        .with("cond", cond)
                        .with("then", Node::List(then))
                        .with("else", otherwise.map(Node::List).unwrap_or(Node::Null))
                }
                "while" => {
                    // a loop body runs after itself: it starts from what holds both before and
                    // after one pass
                    let mut first = slots.clone();
                    self.statements(node.list("body"), &mut first);
                    let mut entry = merged(slots, &[(first, &[]), (slots.clone(), &[])]);
                    let cond = self.expression(&node["cond"], &mut entry);
                    let mut after = entry.clone();
                    let body = self.statements(node.list("body"), &mut after);
                    *slots = merged(&entry, &[(after, &[]), (entry.clone(), &[])]);
                    node.clone().with("cond", cond).with("body", Node::List(body))
                }
                _ => self.expression(node, slots),
            })
            .collect()
    }

    /// Names the slots of the natives in an expression (inner calls first), and follows what
    /// they and the calls do to the slots.
    fn expression(&self, node: &Node, slots: &mut Slots) -> Node {
        let map = match node {
            Node::List(items) => return Node::List(items.iter().map(|item| self.expression(item, slots)).collect()),
            Node::Map(map) if !matches!(node.kind(), "if" | "while") => map,
            other => return other.clone(),
        };
        let mut rebuilt = crate::node::Map::with_capacity(map.len());
        for (key, value) in map.entries() {
            let value = if ["args", "left", "right", "operand", "value", "target", "expr"].contains(&key.as_ref()) {
                self.expression(value, slots)
            } else {
                value.clone()
            };
            rebuilt.insert(key.clone(), value);
        }
        let mut node = Node::Map(rebuilt);
        let name = node.text("name").to_string();
        let found_effects = effects(&node, self.found);
        match (&found_effects, node.kind()) {
            (Some(found_effects), _) => {
                for effect in found_effects {
                    match (effect.slot, &effect.pack) {
                        (Some(slot), Some(pack)) => {
                            slots.insert(slot, pack.clone());
                        }
                        (Some(slot), None) => {
                            slots.remove(&slot);
                        }
                        (None, _) => slots.clear(),
                    }
                }
            }
            (None, "call") if !self.found.inlines.contains_key(&name) => match self.touched.get(&name) {
                Some(Some(changed)) => slots.retain(|slot, _| !changed.contains(slot)),
                _ => slots.clear(),
            },
            _ => {}
        }
        // the slot arguments, named for the character the slot holds here
        let declared = match node.kind() {
            "native" => self.found.natives.get(sdk_name(&node, self.found)),
            "call" => self.found.inlines.get(&name),
            _ => None,
        };
        if let Some(declared) = declared {
            let params = declared.list("params");
            let mut args = node.list("args").to_vec();
            let mut renamed = false;
            for (position, arg) in args.iter_mut().enumerate() {
                let is_slot = params.get(position).is_some_and(|param| param["kind"].as_str() == Some("chara_slot"));
                let Some(value) = arg["value"].as_i64().filter(|_| is_slot && arg.kind() == "int") else {
                    continue;
                };
                if let Some(constant) =
                    slots.get(&value).and_then(|pack| self.found.slot_packs.get(&(value, pack.clone())))
                {
                    *arg = crate::node!({"kind": "var", "name": constant.clone()});
                    renamed = true;
                }
            }
            if renamed {
                node = node.with("args", args);
            }
        }
        node
    }
}

/// The functions with each slot argument a load reaches named for its character. hidden: the
/// library functions the script includes, which its functions may call.
pub fn named_slots(functions: Vec<Node>, hidden: &[Node], found: &Context) -> Vec<Node> {
    if found.slot_packs.is_empty() {
        return functions;
    }
    let all: Vec<&Node> = functions.iter().chain(hidden).filter(|function| function.kind() == "function").collect();
    let touched = summaries(&all, found);
    let pass = Pass { found, touched: &touched };
    functions
        .into_iter()
        .map(|function| {
            if function.kind() != "function" {
                return function;
            }
            let body = pass.statements(function.list("body"), &mut Slots::new());
            function.with("body", Node::List(body))
        })
        .collect()
}
