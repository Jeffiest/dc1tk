//! The SDK: its names, helpers, groups and declarations, from bytes to C and back.
mod common;

use std::collections::{BTreeSet, HashMap, HashSet};

use common::{message, no_texts, tables, texts};
use dc2tk_script::names::Known;
use dc2tk_script::node;
use dc2tk_script::node::Node;
use dc2tk_script::sdk::{self, Context, Declarations, Passed, Sounds};
use dc2tk_script::soundpass::{self, UNKNOWN_BANK};
use dc2tk_script::stb::{self, Region};
use dc2tk_script::{compile, decompile, library, names, parse, source};
use indexmap::IndexMap;

const EVENT_HEADER: &str = r#"#ifndef C2S_EVENT_H
#define C2S_EVENT_H
#include <c2s.h>
#pragma natives event

enum program {
    init = 100,
};

// Reads an event flag.
extern int get_flag(int flag) NATIVE(14);
extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);

static inline void get_flag_twice(int flag, int *value)
{
    *value = get_flag(flag);
    *value = get_flag(*value);
}

static inline void set_actor_face(int actor, string face, string eyes)
{
    set_actor_texture_anim(actor, 0);
    set_actor_texture_anim(actor, 1, face);
    set_actor_texture_anim(actor, 1, eyes);
}
#endif
"#;

const MONSTER_HEADER: &str = "#include <c2s.h>
#pragma natives monster

extern float get_target_angle(void) NATIVE(70, 15);
";

fn headers() -> IndexMap<String, String> {
    texts(&[("event.h", EVENT_HEADER), ("monster.h", MONSTER_HEADER), ("c2s.h", "#define NATIVE(...)\n")])
}

/// HEADERS with one header replaced (dict(HEADERS, **{name: text})).
fn headers_with(name: &str, text: &str) -> IndexMap<String, String> {
    let mut found = headers();
    found.insert(name.to_string(), text.to_string());
    found
}

const EVENT: &str = r#"#include <event.h>

PROGRAM(init, on_init);
MAIN(on_init);

void on_init(void)
{
    int flag;

    flag = get_flag(12);
    set_actor_face(flag, "smile", "blink");
    set_actor_texture_anim(1, 0);
}
"#;

const MONSTER: &str = "#include <monster.h>

PROGRAM(100, program_100);
MAIN(program_100);

void program_100(void)
{
    float target_angle;

    target_angle = get_target_angle();
    AT(60) target_angle = get_target_angle();
    DIRECT target_angle = get_target_angle();
    AT(70) _GET_TARGET_ANGLE(&target_angle);
}
";

fn build_with(text: &str, headers: &IndexMap<String, String>) -> dc2tk_script::Result<Vec<u8>> {
    let program = library::expand(parse::parse(text)?, &no_texts(), headers)?;
    compile::compile_program(&program, tables())
}

fn try_build(text: &str) -> dc2tk_script::Result<Vec<u8>> {
    build_with(text, &headers())
}

fn build(text: &str) -> Vec<u8> {
    try_build(text).unwrap()
}

fn header_context(table: &str, headers: &IndexMap<String, String>) -> Context {
    sdk::header_context(table, headers, tables(), &[], &|text| parse::parse(text)).unwrap().unwrap()
}

/// sdk.present(program, found) with no kinds, hidden functions or sound state.
fn present(program: Node, found: &Context) -> Node {
    sdk::present(program, Some(found), &HashMap::new(), &[], &mut Sounds::default())
}

/// The program with its functions named, from no definitions, around the names found reserves.
fn named(program: Node, found: &Context, program_names: &HashMap<i64, String>) -> Node {
    let known = Known::new(Default::default());
    let lookup = known.with_loose(&IndexMap::new());
    names::apply(program, &lookup, &sdk::reserved(Some(found)), program_names).unwrap()
}

/// The decompiled text of a script with the header names and the program names.
fn shown(data: &[u8], table: &str, headers: &IndexMap<String, String>, program_names: bool) -> String {
    let found = header_context(table, headers);
    let program = decompile::decompile(&stb::read(data), table, tables());
    let programs = if program_names { found.program_names.clone() } else { HashMap::new() };
    source::text(&present(named(program, &found, &programs), &found)).unwrap()
}

fn readable(data: &[u8], table: &str) -> String {
    shown(data, table, &headers(), true)
}

#[test]
fn sdk_names_round_trip_through_bytes() {
    let data = build(EVENT);
    assert_eq!(readable(&data, "event"), EVENT);
}

#[test]
fn inline_helpers_expand_to_native_calls() {
    let plain = EVENT.replace(
        "    set_actor_face(flag, \"smile\", \"blink\");\n",
        "    _OBJS_TEX_ANIME(flag, 0);\n    _OBJS_TEX_ANIME(flag, 1, \"smile\");\n    _OBJS_TEX_ANIME(flag, 1, \"blink\");\n",
    );
    assert_eq!(build(&plain), build(EVENT));
}

#[test]
fn default_groups_and_explicit_forms() {
    let data = build(MONSTER);
    let text = readable(&data, "monster");
    assert!(text.contains(
        "    target_angle = get_target_angle();\n    AT(60) target_angle = get_target_angle();\n    DIRECT target_angle = get_target_angle();\n    target_angle = get_target_angle();\n"
    ));
    assert_eq!(build(&text), data);
}

#[test]
fn original_names_still_compile() {
    let original = EVENT.replace("flag = get_flag(12);", "_GET_FLAG(12, &flag);");
    assert_eq!(build(&original), build(EVENT));
}

#[test]
fn program_names_come_from_the_header() {
    assert_eq!(build(&EVENT.replace("PROGRAM(init, on_init);", "PROGRAM(100, on_init);")), build(EVENT));
    let error = message(try_build(&EVENT.replace("PROGRAM(init, on_init);", "PROGRAM(start, on_init);")));
    assert!(error.contains("start"), "{error}");
}

#[test]
fn locals_are_named_after_results() {
    let renamed = regex::Regex::new(r"\bflag\b").unwrap().replace_all(EVENT, "v0").into_owned();
    assert_eq!(readable(&build(&renamed), "event"), EVENT);
}

#[test]
fn natives_in_expressions_are_called_first() {
    build(&EVENT.replace("flag = get_flag(12);", "flag = get_flag(12) + 1;"));
    let error = message(try_build(&EVENT.replace("flag = get_flag(12);", "flag = set_actor_texture_anim(1, 0);")));
    assert!(error.contains("returns nothing"), "{error}");
}

#[test]
fn header_declarations_parse() {
    let header =
        parse::parse(&format!("{EVENT_HEADER}extern void get_pos(OPTIONAL(float *x, ...)) NATIVE(70, 15);\n")).unwrap();
    let declared = header["declarations"]["natives"].as_array().unwrap();
    assert_eq!(declared[0]["returns"], "int");
    let params = declared[0]["params"].as_array().unwrap();
    assert!(params
        .last()
        .unwrap()
        .same_value(&node!({"name": "flag", "type": "int", "out": true, "optional": false, "result": true})));
    assert!(declared[1]["params"][2]
        .same_value(&node!({"name": "anim_name", "type": "string", "out": false, "optional": true})));
    assert!(declared[2]["base"] == Node::Int(70) && declared[2]["no"] == Node::Int(85));
    assert_eq!(declared[2]["variadic"], Node::Bool(true));
    assert_eq!(declared[2]["params"][0]["out"], Node::Bool(true));
    assert!(header["declarations"]["programs"].same_value(&node!([{"name": "init", "no": 100}])));
}

#[test]
fn pointer_parameters_of_helpers() {
    let plain = EVENT.replace("    flag = get_flag(12);\n", "    flag = get_flag(12);\n    flag = get_flag(flag);\n");
    let folded = EVENT.replace("    flag = get_flag(12);\n", "    get_flag_twice(12, &flag);\n");
    assert_eq!(build(&plain), build(&folded));
    assert_eq!(readable(&build(&plain), "event"), folded);
}

#[test]
fn declarations_print_as_c() {
    let header = parse::parse(EVENT_HEADER).unwrap()["declarations"].clone();
    assert_eq!(source::native_declaration(&header["natives"][0]).unwrap(), "extern int get_flag(int flag) NATIVE(14);");
    assert_eq!(
        source::native_declaration(&header["natives"][1]).unwrap(),
        "extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);"
    );
    let found = sdk::context(&Declarations::of(&header), "event", tables()).unwrap();
    let text = source::inline_text(&sdk::present_inline(&found.inlines["get_flag_twice"], &found)).unwrap().join("\n");
    assert_eq!(
        text,
        "static inline void get_flag_twice(int flag, int *value)\n{\n    *value = get_flag(flag);\n    *value = get_flag(*value);\n}"
    );
}

#[test]
fn void_pointers_are_out_parameters_of_any_type() {
    // clang passes int * and float * to void * but not to any *; both read as an out any
    let native = "extern void get_random(any range, void *value, OPTIONAL(any *other)) NATIVE(1013);\n";
    let header = parse::parse(native).unwrap()["declarations"].clone();
    let params = header["natives"][0]["params"].as_array().unwrap();
    assert!(params[1].same_value(&node!({"name": "value", "type": "any", "out": true, "optional": false})));
    assert!(params[2].same_value(&node!({"name": "other", "type": "any", "out": true, "optional": true})));
    assert_eq!(
        source::native_declaration(&header["natives"][0]).unwrap(),
        "extern void get_random(any range, void *value, OPTIONAL(void *other)) NATIVE(1013);"
    );
}

const CONSTANT_HEADER: &str = "#include <c2s.h>
#pragma natives event

#define PI 3.1415927

enum motion_flags {
    MOTION_ONCE = 2,
    MOTION_LOOP = 4,
};

extern void set_actor_motion(int actor, string motion, enum motion_flags flags) NATIVE(465);
extern void wait_frames_native(int frames) NATIVE(14);
";

const CONSTANTS: &str = r#"#include <event.h>

PROGRAM(100, program_100);
MAIN(program_100);

void program_100(void)
{
    float angle;

    angle = PI * 2.0;
    set_actor_motion(0, "walk", MOTION_LOOP | MOTION_ONCE);
    set_actor_motion(0, "walk", 4);
    set_actor_motion(0, "walk", 6);
    wait_frames_native(4);
}
"#;

fn constant_headers() -> IndexMap<String, String> {
    texts(&[("event.h", CONSTANT_HEADER)])
}

fn constant_build(text: &str) -> Vec<u8> {
    build_with(text, &constant_headers()).unwrap()
}

#[test]
fn constants_compile_to_their_literals() {
    let literal = CONSTANTS.replace("PI * 2.0", "3.1415927 * 2.0").replace("MOTION_LOOP | MOTION_ONCE", "4 | 2");
    assert_eq!(constant_build(CONSTANTS), constant_build(&literal));
}

#[test]
fn a_constant_declared_twice_is_refused() {
    let content = |members: &str| {
        let pal = format!("enum base_se_pal {{\n    SE_STEP_CARPET_L = 8,\n{members}}};\n#define SIDE 1\n");
        let mut headers = constant_headers();
        headers.insert("pal.h".into(), pal);
        sdk::header_context("event", &headers, tables(), &["pal.h".into()], &|text| parse::parse(text))
    };
    let error = content("    SE_STEP_CARPET_L = 42,\n").unwrap_err().to_string();
    assert!(
        error.contains("SE_STEP_CARPET_L is declared twice (= 8 in enum base_se_pal, then = 42 in enum base_se_pal)")
    );
    // also against a member of another enum or a #define, and a #define given another value
    assert!(content("};\nenum other_se_pal {\n    SE_STEP_CARPET_L = 8,\n").is_err());
    assert!(content("    SIDE = 1,\n").is_err());
    assert!(content("};\n#define SIDE 2\nenum x {\n").is_err());
    // a #define repeated with its own value is valid C
    assert!(content("};\n#define SIDE 1\nenum x {\n").is_ok());
    assert!(content("    SE_STEP_CARPET_ON_STONE_L = 42,\n").is_ok());
    // a compile reads the same declarations
    let script = "#include <event.h>\n#include <twice.h>\nMAIN(f);\nvoid f(void)\n{\n}\n";
    let mut headers = constant_headers();
    headers.insert("twice.h".into(), "enum a { A = 1 };\nenum b { A = 2 };\n".into());
    assert!(build_with(script, &headers).unwrap_err().to_string().contains("A is declared twice"));
}

#[test]
fn an_address_passed_for_an_enum_is_cast_to_it() {
    let cast = CONSTANTS
        .replace("set_actor_motion(0, \"walk\", 6);", "set_actor_motion(0, \"walk\", (enum motion_flags)&angle);");
    let loose = constant_headers()["event.h"].replace("enum motion_flags flags", "any flags");
    let plain = cast.replace("(enum motion_flags)&angle", "&angle");
    // the cast compiles to nothing: the same bytes as passing the address where any is taken
    assert_eq!(constant_build(&cast), build_with(&plain, &texts(&[("event.h", &loose)])).unwrap());
    let text = shown(&constant_build(&cast), "event", &constant_headers(), false);
    assert!(text.contains("    set_actor_motion(0, \"walk\", (enum motion_flags)&v0);\n"), "{text}");
    assert_eq!(constant_build(&text), constant_build(&cast));
    assert!(build_with(&cast.replace("&angle);", "angle);"), &constant_headers()).is_err());
}

#[test]
fn decompiler_names_only_enum_typed_literals() {
    let text = shown(&constant_build(CONSTANTS), "event", &constant_headers(), false);
    assert!(text.contains("    set_actor_motion(0, \"walk\", MOTION_LOOP | MOTION_ONCE);\n"));
    assert!(text.contains("    set_actor_motion(0, \"walk\", MOTION_LOOP);\n"));
    assert!(text.contains("    set_actor_motion(0, \"walk\", 6);\n"));
    assert!(text.contains("    wait_frames_native(4);\n"));
    assert!(text.contains(" = PI * 2.0;\n"));
    assert_eq!(constant_build(&text), constant_build(CONSTANTS));
}

#[test]
fn locals_shadow_constants() {
    let shadowed = CONSTANTS
        .replace("    float angle;\n", "    float angle;\n    int MOTION_LOOP;\n")
        .replace("set_actor_motion(0, \"walk\", 4);", "set_actor_motion(0, \"walk\", MOTION_LOOP);");
    let script = stb::read(&constant_build(&shadowed));
    let Region::Function(main) = &script.regions[2] else { panic!("no main") };
    assert!(main.instructions.iter().any(|item| item.name() == "LOAD" && item.arg1 == 1));
}

#[test]
fn raw_numbered_calls_name_the_native_they_were_written_for() {
    let headers = headers_with(
        "monster.h",
        &format!("{MONSTER_HEADER}extern void get_monster_var2(int index, OPTIONAL(int *value)) NATIVE(35, 25);\n"),
    );
    let text = MONSTER.replace("    AT(60) target_angle = get_target_angle();\n", "    DIRECT get_monster_var2(1);\n");
    let data = build_with(&text, &headers).unwrap();
    let shown = shown(&data, "monster", &headers, true);
    // raw 60 is get_target_angle in the table the call was written for
    assert!(
        shown.contains("    DIRECT get_monster_var2(1);  // old numbering, meant as get_target_angle\n"),
        "{shown}"
    );
    assert!(shown.contains("    DIRECT target_angle = get_target_angle();\n"));
}

#[test]
fn scripts_built_for_the_old_table_read_current_groups_as_old() {
    let headers = headers_with(
        "monster.h",
        &format!(
            "{MONSTER_HEADER}extern void set_visible_monster_limit(int count, OPTIONAL(int extra2)) NATIVE(110, 9);\nextern void set_mask_flag(int mask, int on) NATIVE(120, 9);\n"
        ),
    );
    let text =
        MONSTER.replace("    AT(60) target_angle = get_target_angle();\n", "    set_visible_monster_limit(4, 1);\n");
    let data = build_with(&text, &headers).unwrap();
    let found = header_context("monster", &headers);
    let program = decompile::decompile(&stb::read(&data), "monster", tables());
    let named = named(program, &found, &found.program_names);
    assert!(!source::text(&present(named.clone(), &found)).unwrap().contains("old numbering"));
    let old = sdk::numbered(Some(found.clone()), Some("old")).unwrap();
    let shown = source::text(&present(named, &old)).unwrap();
    assert!(
        shown.contains("    set_visible_monster_limit(4, 1);  // old numbering, meant as set_mask_flag\n"),
        "{shown}"
    );
}

#[test]
fn sound_entries_take_the_names_of_the_bank_their_id_came_from() {
    let header = EVENT_HEADER.replace(
        "#endif",
        "extern int get_system_sound_id(void) NATIVE(620);\nextern void play_se(int sound_id, int se_no, OPTIONAL(int volume, int pan)) NATIVE(602);\nenum system_se_pal {\n    SE_TREASURE_GET = 18,\n};\n#endif",
    );
    let headers = headers_with("event.h", &header);
    let text = EVENT.replace(
        "    set_actor_texture_anim(1, 0);\n",
        "    flag = get_system_sound_id();\n    play_se(flag, 18);\n    flag = 3;\n    play_se(flag, 18);\n",
    );
    let data = build_with(&text, &headers).unwrap();
    let shown = shown(&data, "event", &headers, true);
    assert!(shown.contains("play_se(flag, SE_TREASURE_GET);"), "{shown}");
    assert!(shown.contains("play_se(flag, 18);"));
}

#[test]
fn unregistered_numbers_of_an_old_table_name_their_native() {
    let mut found = Context::default();
    found.raw_table = true;
    found.old_direct.insert(80, 76);
    found.by_number.insert(76, "add_motion_se".into());
    let call = node!({"name": "_EXT_80", "base": null});
    assert_eq!(sdk::meant_native(&call, &found).as_deref(), Some("add_motion_se"));
    found.raw_table = false;
    assert_eq!(sdk::meant_native(&call, &found), None);
}

fn var(name: &str) -> Node {
    node!({"kind": "var", "name": name})
}

fn num(value: i64) -> Node {
    node!({"kind": "int", "value": value})
}

/// A context with sound bank enums: {enum: {value: name}}.
fn sound_context(enums: &[(&str, i64, &str)], sound_set_banks: &[(i64, &str)]) -> Context {
    let mut found = Context::default();
    for (name, value, member) in enums {
        found.enums.entry(name.to_string()).or_default().insert(*value, member.to_string());
    }
    found.sound_set_banks = sound_set_banks.iter().map(|(number, bank)| (*number, bank.to_string())).collect();
    found
}

/// The sound pass over statements, with no banks known to start with.
fn named_sounds(nodes: &[Node], found: &Context) -> Node {
    soundpass::named_sounds(nodes, found, &mut soundpass::Banks::default(), None, None)
}

#[test]
fn entries_of_loaded_banks_take_their_bank_names() {
    let found = sound_context(
        &[("se_sp_000_pal", 0, "SE_SP_000_FANFARE"), ("se_ev_283_000_pal", 3, "SE_EV_283_000_DOOR")],
        &[(2237, "EV_283_000")],
    );
    let body = vec![
        node!({"kind": "native", "name": "load_sound", "args": [var("SND_PORT_MENU"), {"kind": "string", "text": "snd2/sp/sp_000.snd"}],
               "result": var("menu")}),
        node!({"kind": "native", "name": "play_se", "args": [var("menu"), num(0)]}),
        node!({"kind": "native", "name": "load_sound", "args": [num(2237), {"kind": "addr", "target": var("scene")}]}),
        node!({"kind": "native", "name": "play_se", "args": [var("scene"), num(3)]}),
        node!({"kind": "native", "name": "get_sound_port_bank", "args": [var("SND_PORT_EVENT")], "result": var("port")}),
        node!({"kind": "native", "name": "play_actor_se", "args": [num(1), var("port"), num(3)]}),
    ];
    let shown = named_sounds(&body, &found);
    assert_eq!(shown[1]["args"][1], var("SE_SP_000_FANFARE"));
    assert_eq!(shown[3]["args"][1], var("SE_EV_283_000_DOOR"));
    assert_eq!(shown[5]["args"][2], var("SE_EV_283_000_DOOR"));
}

/// A call statement as the decompiler makes it: an expression.
fn call(name: &str, args: Vec<Node>) -> Node {
    node!({"kind": "expr", "expr": {"kind": "call", "name": name, "args": args}})
}

fn key(name: &str, position: usize) -> (String, usize) {
    (name.to_string(), position)
}

#[test]
fn banks_reach_entries_through_parameters_and_out_parameters() {
    let found = sound_context(&[("se_sp_000_pal", 0, "SE_SP_000_FANFARE")], &[]);
    let load = node!({"kind": "native", "name": "load_sound",
                      "args": [var("SND_PORT_MENU"), {"kind": "string", "text": "snd2/sp/sp_000.snd"}, var("out")]});
    let loader = node!({"kind": "function", "name": "load_fanfare", "params": [{"name": "out", "array": true}], "locals": [],
                        "body": [load]});
    let player = node!({"kind": "function", "name": "play_fanfare", "params": [{"name": "bank", "array": false}], "locals": [],
                        "body": [{"kind": "native", "name": "play_se", "args": [var("bank"), {"kind": "int", "value": 0}]}]});
    let caller = node!({"kind": "function", "name": "on_start", "params": [], "locals": [{"name": "id"}],
                        "body": [call("load_fanfare", vec![node!({"kind": "addr", "target": var("id")})]),
                                 call("play_fanfare", vec![var("id")])]});
    let (known, _) = soundpass::program_sounds(&[loader.clone(), player.clone(), caller.clone()], &found, None, None);
    let mut banks = soundpass::sound_banks(&player, &known);
    let flow = soundpass::sound_flow(&player, &known);
    let body = player["body"].as_array().unwrap();
    let shown = soundpass::named_sounds(body, &found, &mut banks, Some(&flow), None);
    assert_eq!(shown[0]["args"][1], var("SE_SP_000_FANFARE"));
    // a caller that passes something the pass cannot follow leaves the parameter unsettled
    let other = node!({"kind": "function", "name": "on_end", "params": [], "locals": [{"name": "id"}],
                       "body": [{"kind": "expr", "expr": {"kind": "assign", "target": var("id"), "value": {"kind": "call", "name": "get", "args": []}}},
                                call("play_fanfare", vec![var("id")])]});
    let (known, _) = soundpass::program_sounds(&[loader, player, caller, other], &found, None, None);
    assert!(!known.params.contains_key(&key("play_fanfare", 0)));
}

#[test]
fn a_library_takes_the_banks_every_script_that_includes_it_passes() {
    let found = sound_context(&[("se_sp_000_pal", 0, "SE_SP_000_FANFARE")], &[]);
    let load = node!({"kind": "native", "name": "load_sound",
                      "args": [var("SND_PORT_MENU"), {"kind": "string", "text": "snd2/sp/sp_000.snd"}], "result": var("id")});
    let helper = node!({"kind": "function", "name": "play_fanfare", "params": [{"name": "bank", "array": false}], "locals": [],
                        "body": [{"kind": "native", "name": "play_se", "args": [var("bank"), {"kind": "int", "value": 0}]}]});
    let caller = node!({"kind": "function", "name": "on_start", "params": [], "locals": [{"name": "id"}],
                        "body": [load, {"kind": "expr", "expr": {"kind": "call", "name": "play_fanfare", "args": [var("id")]}}]});
    let (known, seen) = soundpass::program_sounds(&[caller, helper.clone()], &found, None, None);
    let passed = soundpass::passed_sounds(std::slice::from_ref(&helper), &known, &seen);
    let expected: IndexMap<(String, usize), BTreeSet<String>> =
        [(key("play_fanfare", 0), BTreeSet::from(["se_sp_000".to_string()]))].into_iter().collect();
    assert_eq!(passed.params, expected);
    let seed = sdk::library_sounds(&[passed.clone(), passed.clone()]);
    let (known, _) = soundpass::program_sounds(std::slice::from_ref(&helper), &found, Some(&seed), None);
    assert_eq!(known.params, HashMap::from([(key("play_fanfare", 0), "se_sp_000".to_string())]));
    // a script that passes something else leaves the library's parameter a number
    let other = Passed {
        params: [(key("play_fanfare", 0), BTreeSet::from([UNKNOWN_BANK.to_string()]))].into_iter().collect(),
        ..Passed::default()
    };
    let seed = sdk::library_sounds(&[passed, other]);
    let (known, _) = soundpass::program_sounds(std::slice::from_ref(&helper), &found, Some(&seed), None);
    assert!(known.params.is_empty());
}

#[test]
fn banks_after_an_if_are_those_its_paths_agree_on() {
    let found = sound_context(&[("se_sp_000_pal", 0, "SE_SP_000_FANFARE")], &[]);
    let load = |path: &str| {
        node!({"kind": "native", "name": "load_sound",
               "args": [var("SND_PORT_MENU"), {"kind": "string", "text": path}], "result": var("id")})
    };
    let play = node!({"kind": "native", "name": "play_se", "args": [var("id"), {"kind": "int", "value": 0}]});
    let branch = |otherwise: Node| node!({"kind": "if", "cond": var("x"), "then": [load("snd2/sp/sp_000.snd")], "else": otherwise});
    // only one path loads: on the other the id holds no bank
    assert_eq!(named_sounds(&[branch(Node::Null), play.clone()], &found)[1]["args"][1], var("SE_SP_000_FANFARE"));
    // the paths load different banks
    let different = named_sounds(&[branch(node!([load("snd2/sp/sp_001.snd")])), play.clone()], &found);
    assert_eq!(different[1]["args"][1].kind(), "int");
    // the path that loads the other bank returns
    let returning = node!([load("snd2/sp/sp_001.snd"), {"kind": "return", "value": null}]);
    assert_eq!(named_sounds(&[branch(returning), play], &found)[1]["args"][1], var("SE_SP_000_FANFARE"));
}

#[test]
fn locals_called_unused_that_are_read_get_a_true_name() {
    let mut found = Context::default();
    found
        .by_original
        .insert("_SET_MODE".into(), node!({"name": "set_mode", "params": [{"name": "mode", "out": false}]}));
    let body = node!([
        {"kind": "native", "name": "_SET_MODE", "base": null, "args": [var("unused2")]},
        {"kind": "expr", "expr": {"kind": "assign", "target": var("unused3"), "value": {"kind": "int", "value": 1}}},
        {"kind": "native", "name": "_SET_MODE", "base": null, "args": [var("unused3")]},
        {"kind": "expr", "expr": {"kind": "assign", "target": var("unused4"), "value": {"kind": "int", "value": 1}}}
    ]);
    let locals: Vec<Node> = ["unused2", "unused3", "unused4"]
        .iter()
        .map(|name| node!({"name": *name, "type": "int", "size": null}))
        .collect();
    let function = node!({"params": [], "locals": locals, "body": body});
    let renamed = |function_names: &[&str]| -> Vec<String> {
        let function_names: HashSet<String> = function_names.iter().map(|name| name.to_string()).collect();
        sdk::named_locals(function.clone(), &found, &function_names)["locals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|declared| declared["name"].as_str().unwrap().to_string())
            .collect()
    };
    // read and never written; written and read; written only (still unused)
    assert_eq!(renamed(&[]), ["unset_mode", "mode", "unused4"]);
    // nor do they take the name of one of the script's functions
    assert_eq!(renamed(&["mode"]), ["unset_mode", "mode_2", "unused4"]);
}

/// The named body of one function after the program pass over all of them.
fn sounds_in(functions: &[Node], entries: &[&str], found: &Context, name: &str) -> Node {
    let entries: HashSet<String> = entries.iter().map(|entry| entry.to_string()).collect();
    let (known, _) = soundpass::program_sounds(functions, found, None, Some(&entries));
    let function = functions.iter().find(|function| function["name"].as_str() == Some(name)).unwrap();
    let mut banks = soundpass::sound_banks(function, &known);
    let flow = soundpass::sound_flow(function, &known);
    soundpass::named_sounds(function["body"].as_array().unwrap(), found, &mut banks, Some(&flow), None)
}

fn function(name: &str, body: Node) -> Node {
    node!({"kind": "function", "name": name, "params": [], "locals": [], "body": body})
}

fn load_event(path: &str) -> Node {
    node!({"kind": "native", "name": "load_sound",
           "args": [var("SND_PORT_EVENT"), {"kind": "string", "text": path}]})
}

fn read_port() -> Node {
    node!({"kind": "native", "name": "get_sound_port_bank", "args": [var("SND_PORT_EVENT")], "result": var("port")})
}

fn stop_port_entry(entry: i64) -> Node {
    node!({"kind": "native", "name": "stop_se", "args": [var("port"), num(entry)]})
}

#[test]
fn the_event_port_follows_calls_and_rests_at_the_bank_events_put_back() {
    let mut found = sound_context(
        &[("se_ev_200_000_pal", 0, "SE_EV_200_000_BATTLE_MUSIC"), ("se_ev_100_105_pal", 11, "SE_EV_100_105_DOOR")],
        &[],
    );
    found.constants.insert("SND_PORT_EVENT".into(), num(4));
    let restore = function("load_event_sound", node!([load_event("snd2/event/EV_200_000.snd")]));
    // a scene loads its own bank, plays from it after a call that keeps the port, and puts the
    // battle music back
    let scene = function(
        "scene",
        node!([
            load_event("snd2/event/EV_100_105.snd"),
            call("keep", vec![]),
            read_port(),
            stop_port_entry(11),
            call("load_event_sound", vec![])
        ]),
    );
    let keep = function("keep", node!([]));
    let victory = function("victory", node!([read_port(), stop_port_entry(0)]));
    let functions = [restore, scene, keep, victory];
    let entries = ["scene", "victory"];
    assert_eq!(sounds_in(&functions, &entries, &found, "scene")[3]["args"][1], var("SE_EV_100_105_DOOR"));
    assert_eq!(sounds_in(&functions, &entries, &found, "victory")[1]["args"][1], var("SE_EV_200_000_BATTLE_MUSIC"));
    // an event that leaves its bank there, and stays on the map, leaves the resting bank unknown
    let leaves = function("leaves", node!([load_event("snd2/event/EV_100_105.snd")]));
    let functions = [functions.to_vec(), vec![leaves]].concat();
    let shown = sounds_in(&functions, &["scene", "victory", "leaves"], &found, "victory");
    assert_eq!(shown[1]["args"][1], num(0));
    // unless it asks to leave the map
    let jumps = function(
        "leaves",
        node!([load_event("snd2/event/EV_100_105.snd"),
               {"kind": "native", "name": "jump_to_map", "args": [num(-1), num(3)]}]),
    );
    let functions = [functions[..4].to_vec(), vec![jumps]].concat();
    let shown = sounds_in(&functions, &["scene", "victory", "leaves"], &found, "victory");
    assert_eq!(shown[1]["args"][1], var("SE_EV_200_000_BATTLE_MUSIC"));
}

#[test]
fn an_unknown_event_port_read_with_the_battle_music_volume_is_the_battle_music() {
    let found = sound_context(&[("se_event_port_pal", 0, "SE_EVENT_PORT_BATTLE_MUSIC")], &[]);
    let volume = node!({"kind": "native", "name": "get_battle_bgm_volume", "args": [], "result": var("volume")});
    let game_over = function("game_over", node!([read_port(), volume, stop_port_entry(0)]));
    let other = function("other", node!([read_port(), stop_port_entry(0)]));
    let functions = [game_over, other];
    assert_eq!(sounds_in(&functions, &[], &found, "game_over")[2]["args"][1], var("SE_EVENT_PORT_BATTLE_MUSIC"));
    assert_eq!(sounds_in(&functions, &[], &found, "other")[1]["args"][1], num(0));
}

#[test]
fn a_bank_set_under_a_condition_is_known_under_the_same_condition() {
    let found = sound_context(&[("se_en_900_pal", 19, "SE_EN_900_VANISH")], &[]);
    let load = |path: &str| {
        node!({"kind": "native", "name": "load_sound",
               "args": [var("SND_PORT_MENU"), {"kind": "string", "text": path}], "result": var("bank")})
    };
    let holed = node!({"kind": "binary", "op": "==", "left": var("holed"), "right": num(1)});
    let minus_one = node!({"kind": "unary", "op": "-", "operand": num(1)});
    let shot = node!({"kind": "function", "name": "shot", "params": [], "locals": [{"name": "bank"}, {"name": "holed"}],
        "body": [load("snd2/mon/EN_900.snd"),
                 {"kind": "if", "cond": holed.clone(), "then": [load("snd2/sp/sp_000.snd")], "else": null},
                 {"kind": "if", "cond": holed, "then": [call("consume", vec![minus_one])],
                  "else": [call("consume", vec![var("bank")])]}]});
    let consume = node!({"kind": "function", "name": "consume", "params": [{"name": "bank", "array": false}], "locals": [],
        "body": [{"kind": "native", "name": "play_se", "args": [var("bank"), num(19)]}]});
    let functions = [shot, consume];
    assert_eq!(sounds_in(&functions, &["shot"], &found, "consume")[0]["args"][1], var("SE_EN_900_VANISH"));
}
