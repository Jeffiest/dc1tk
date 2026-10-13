//! src/tests/test_source.py

mod common;

use c2_script::definitions::resolved_posix;
use c2_script::node::Node;
use c2_script::stb::{self, Region};
use c2_script::{asm, compile, decompile, parse, sdk, source};
use common::{default_set, message, sample_script, sdk_dir, tables};

const EVENT: &str = r#"#pragma natives event

int g0;
float g1;

PROGRAM(100, f_0000);
PROGRAM(200, f_04d4);
MAIN(f_0000);

void f_0000(void)
{
    int v0;
    float v1;
    int v2[4];

    v0 = 3;
    v1 = 0.5;
    v2[v0] = v0 = 2;
    g0 = g0 + 1;
    g1 = sin(v1) * -2.0;
    _GET_FLAG(12, &v0);
    if (v0 == 1 && g0 > 2 || !v0) {
        print("hello", v0);
    } else if (v0 < 0) {
        wait;
    } else {
        skip_end;
    }
    while (1) {
        if (v2[0] >= 4) {
            break;
        }
        if (v2[1] != 4) {
            continue;
        }
        v2[0] = f_04d4(&v2[0], (float)v0) - (v0 - 1);
    }
    end;
}

int f_04d4(int a0[], float a1)
{
    a0[0] = (int)a1 % 7;
    return a0[0] & 1 | 2;
}
"#;

const NAMED: &str = r#"#pragma natives event

int visits;
float speed;

PROGRAM(100, start);
PROGRAM(200, helper);
MAIN(start);

void start(void)
{
    int count;
    float angle;
    int slots[4];

    count = 3;
    angle = 0.5;
    slots[count] = count = 2;
    visits = visits + 1;
    speed = sin(angle) * -2.0;
    _GET_FLAG(12, &count);
    if (count == 1 && visits > 2 || !count) {
        print("hello", count);
    } else if (count < 0) {
        wait;
    } else {
        skip_end;
    }
    while (1) {
        if (slots[0] >= 4) {
            break;
        }
        if (slots[1] != 4) {
            continue;
        }
        slots[0] = helper(&slots[0], (float)count) - (count - 1);
    }
    end;
}

int helper(int values[], float scale)
{
    values[0] = (int)scale % 7;
    return values[0] & 1 | 2;
}
"#;

const MONSTER: &str = "#pragma natives monster

PROGRAM(100, f_0000);
MAIN(f_0000);

void f_0000(void)
{
    float v0;

    AT(70) _GET_TARGET_ANGLE(&v0);
    AT(170) _SET_MOS(3, 1);
    _SET_MOS(3, 1);
    _EXT_190(v0);
}
";

fn build_text(text: &str) -> c2_script::Result<Vec<u8>> {
    compile::compile_program(&parse::parse(text)?, tables())
}

fn built(text: &str) -> Vec<u8> {
    build_text(text).unwrap()
}

fn decompiled(data: &[u8], table: &str) -> String {
    source::text(&decompile::decompile(&stb::read(data), table, tables())).unwrap()
}

fn disassembled(data: &[u8], table: &str) -> String {
    asm::disassemble(&stb::read(data), table, tables())
}

/// Parsed statements carry their source line, for error messages; the decompiler's have none.
fn without_lines(node: &Node) -> Node {
    match node {
        Node::List(items) => Node::List(items.iter().map(without_lines).collect()),
        Node::Map(map) => {
            let mut out = c2_script::node::Map::new();
            for (key, value) in map.iter() {
                if key != "line" && key != "start" {
                    out.insert(key.to_string(), without_lines(value));
                }
            }
            Node::Map(out)
        }
        other => other.clone(),
    }
}

fn sorted_keys(node: &Node) -> Vec<String> {
    let mut keys: Vec<String> = node.as_object().unwrap().keys().map(str::to_string).collect();
    keys.sort();
    keys
}

#[test]
fn source_round_trips_through_bytes() {
    let data = built(EVENT);
    assert_eq!(decompiled(&data, "event"), EVENT);
    let resolved = sdk::resolve(&parse::parse(EVENT).unwrap(), tables()).unwrap();
    let mut tree = decompile::decompile(&stb::read(&data), "event", tables());
    let map = tree.as_object_mut().unwrap();
    let fingerprints = map.remove("fingerprints").unwrap();
    let loose = map.remove("loose_fingerprints").unwrap();
    assert_eq!(sorted_keys(&fingerprints), ["f_0000", "f_04d4"]);
    assert_eq!(sorted_keys(&loose), ["f_0000", "f_04d4"]);
    let (resolved, tree) = (without_lines(&resolved), without_lines(&tree));
    assert!(resolved.py_eq(&tree), "{resolved}\n{tree}");
}

#[test]
fn names_are_free_to_choose() {
    assert_eq!(built(NAMED), built(EVENT));
}

#[test]
fn grouped_and_unregistered_natives() {
    let data = built(MONSTER);
    assert_eq!(decompiled(&data, "monster"), MONSTER);
    let text = disassembled(&data, "monster");
    assert!(text.contains("    native _GET_DIST\n    push 15\n    add\n"));
    assert!(text.contains("    native _SET_DAMAGE_SCORE\n    push 12\n    add\n"));
    assert!(text.contains("    native _SET_MOS\n"));
    assert!(text.contains("    native 190\n"));
}

#[test]
fn negation_is_an_instruction() {
    assert!(disassembled(&built(EVENT), "event").contains("    push 2.0\n    neg\n"));
}

#[test]
fn labels_follow_allocation_order() {
    let text = disassembled(&built(EVENT), "event");
    let numbers: Vec<i64> = text
        .split('\n')
        .filter(|line| line.starts_with("    nop "))
        .map(|line| line.split_whitespace().nth(1).unwrap().parse().unwrap())
        .collect();
    assert_eq!(numbers, [0, 1, 2, 4, 5, 3, 6, 8, 9, 7]);
}

#[test]
fn unused_strings_and_padding_pragmas() {
    let text = EVENT.replace(
        "void f_0000(void)",
        "#pragma strings \"old\" \"older\"\n#pragma padding 01020304\nvoid f_0000(void)",
    );
    let data = built(&text);
    let Region::Function(main) = &stb::read(&data).regions[2] else { panic!("no main") };
    assert_eq!(main.strings, ["old", "older", "hello"]);
    assert_eq!(main.padding, [1, 2, 3, 4]);
    let again = decompiled(&data, "event");
    assert!(again.contains("#pragma strings \"old\" \"older\"\nvoid f_0000(void)"));
    assert!(again.ends_with("\n#pragma padding_of f_0000 01020304\n"));
    assert_eq!(built(&again), data);
}

#[test]
fn whole_file_assembly_fallback() {
    let data = sample_script();
    let text = decompiled(&data, "event");
    assert_eq!(text.matches("\nasm f_").count(), 2);
    assert_eq!(built(&text), data);
}

#[test]
fn errors_name_the_line() {
    for (broken, expected) in [
        ("void f_0000(void)\n{\n    v0 = ;\n}\n", "line 3"),
        ("MAIN(f_0000);\nvoid f_0000(void)\n{\n    wait\n}\n", "line 5"),
        ("MAIN(f_0000);\nvoid f_0000(void)\n{\n    print(\"a);\n}\n", "line 4"),
    ] {
        let error = message(parse::parse(broken));
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn unknown_names_fail_to_compile() {
    for broken in [
        EVENT.replace("_GET_FLAG", "_NO_SUCH_NATIVE"),
        EVENT.replace("g0 = g0 + 1", "g9 = 1"),
        EVENT.replace("f_04d4(&v2[0], (float)v0)", "f_04d4(1)"),
    ] {
        assert!(build_text(&broken).is_err(), "compiled a broken program");
    }
}

#[test]
fn fingerprints_ignore_names_and_position() {
    let prints = |text: &str| decompile::decompile(&stb::read(&built(text)), "event", tables())["fingerprints"].clone();
    let values = |node: &Node| {
        let mut found: Vec<String> = node.as_object().unwrap().values().map(|value| value.to_string()).collect();
        found.sort();
        found
    };
    let first = prints(EVENT);
    let second = prints(NAMED);
    assert_eq!(values(&first), values(&second));
    // (the Python also replaces "int g0;\n" with itself, which changes nothing)
    let changed = prints(&EVENT.replace("    v0 = 3;\n", "    v0 = 4;\n"));
    assert_ne!(changed["f_0000"], first["f_0000"]);
    assert_eq!(changed["f_04d4"], first["f_04d4"]);
}

#[test]
fn editor_flags_point_clang_at_the_sdk_the_sets_and_the_tree() {
    let text = source::editor_flags(&sdk_dir(), &[default_set()], None);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[..3], ["-xc", "-std=c2x", "-m32"]);
    assert!(lines.contains(&format!("-I{}", resolved_posix(&sdk_dir())).as_str()));
    assert!(lines.contains(&format!("-I{}", resolved_posix(&default_set().join("include"))).as_str()));
    assert_eq!(lines[lines.len() - 2..], ["-Iinclude", "-I."]);
}

#[test]
fn docs_read_back_as_written() {
    let doc = "Plays a back flip, pushing backwards while the flip is mid-motion; ".repeat(3) + "\nA second paragraph.";
    let lines = source::doc_lines(&doc);
    let comments: Vec<(usize, String)> =
        lines.iter().enumerate().map(|(at, line)| (at + 1, c2_py::strip(&line[2..]).to_string())).collect();
    assert_eq!(parse::comment_block(&comments, lines.len() + 1), c2_py::strip(&doc.replace("; \n", ";\n")));
}

#[test]
fn a_program_naming_no_function_says_so() {
    let error = message(build_text(&EVENT.replace("PROGRAM(200, f_04d4);", "PROGRAM(200, renamed_away);")));
    assert_eq!(error, "PROGRAM(200, renamed_away): no function renamed_away");
}

#[test]
fn a_string_pool_file_round_trips_without_pragmas_per_function() {
    let text = EVENT.replacen("#pragma natives event\n", "#pragma natives event\n#pragma string_pool\n", 1);
    let data = built(&text);
    let again = decompiled(&data, "event");
    assert!(again.contains("#pragma string_pool\n"));
    assert!(!again.contains("#pragma strings") && !again.contains("#pragma padding_of"));
    assert_eq!(built(&again), data);
}
