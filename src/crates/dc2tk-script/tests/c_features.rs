//! C's features beyond the forms the decompiler writes, compiled and run in the test VM.

mod common;

use common::{run_function, sdk_dir, sdk_texts, tables, texts, write, TempDir};
use dc2tk_script::vm::{self, Value};
use dc2tk_script::{compile, library, parse};

const HEAD: &str = "#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\n";
const EVENT: &str = "#include <event.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\n";

fn built(text: &str, head: &str) -> dc2tk_script::Result<Vec<u8>> {
    let program = library::expand(parse::parse(&format!("{head}{text}"))?, &common::no_texts(), sdk_texts())?;
    compile::compile_program(&program, tables())
}

fn same(text: &str, plain: &str, head: &str) {
    assert_eq!(built(text, head).unwrap(), built(plain, head).unwrap(), "{text}");
}

fn result(text: &str, args: &[i128]) -> Value {
    let args: Vec<Value> = args.iter().map(|value| Value::Int(*value)).collect();
    common::run(&format!("{HEAD}{text}"), "f", &args).unwrap()
}

fn fails(text: &str, expected: &str) {
    match built(text, HEAD) {
        Ok(_) => panic!("compiled: {text}"),
        Err(error) => assert!(error.to_string().contains(expected), "{error}"),
    }
}

fn int(value: i128) -> Value {
    Value::Int(value)
}

// Features that are a different way to write code the compiler already took compile to the same
// bytes as that code.

#[test]
fn initializers_and_declarations_anywhere() {
    same(
        "void f(void)\n{\n    int a = 1;\n    wait;\n    float b = 2.0;\n    int c[3] = {4, 5};\n}\n",
        "void f(void)\n{\n    int a;\n    float b;\n    int c[3];\n\n    a = 1;\n    wait;\n    b = 2.0;\n    c[0] = 4;\n    c[1] = 5;\n    c[2] = 0;\n}\n",
        HEAD,
    );
    same(
        "void f(void)\n{\n    int a[] = {7, 8};\n\n    a[1] = 0;\n}\n",
        "void f(void)\n{\n    int a[2];\n\n    a[0] = 7;\n    a[1] = 8;\n    a[1] = 0;\n}\n",
        HEAD,
    );
}

#[test]
fn for_loops_are_while_loops_that_step_before_continue() {
    same(
        "void f(void)\n{\n    for (int i = 0; i < 3; i++) {\n        if (i == 1) {\n            continue;\n        }\n        wait;\n    }\n}\n",
        "void f(void)\n{\n    int i;\n\n    i = 0;\n    while (i < 3) {\n        if (i == 1) {\n            i = i + 1;\n            continue;\n        }\n        wait;\n        i = i + 1;\n    }\n}\n",
        HEAD,
    );
}

#[test]
fn do_loops_test_after_the_body() {
    same(
        "void f(void)\n{\n    int i;\n\n    do {\n        wait;\n    } while (i);\n}\n",
        "void f(void)\n{\n    int i;\n\n    while (1) {\n        wait;\n        if (!i) {\n            break;\n        }\n    }\n}\n",
        HEAD,
    );
}

#[test]
fn compound_assignment_steps_and_braceless_bodies() {
    same(
        "void f(void)\n{\n    int i;\n\n    i += 2;\n    i *= 3;\n    ++i;\n    i--;\n    if (i) wait;\n    else end;\n}\n",
        "void f(void)\n{\n    int i;\n\n    i = i + 2;\n    i = i * 3;\n    i = i + 1;\n    i = i - 1;\n    if (i) {\n        wait;\n    } else {\n        end;\n    }\n}\n",
        HEAD,
    );
}

#[test]
fn literals_blocks_and_empty_statements() {
    same(
        "void f(void)\n{\n    int c;\n    float x;\n\n    c = 'A' + '\\n';\n    x = 2.5f;\n    { ; }\n}\n",
        "void f(void)\n{\n    int c;\n    float x;\n\n    c = 65 + 10;\n    x = 2.5;\n}\n",
        HEAD,
    );
}

#[test]
fn preprocessor_conditions_choose_code() {
    same(
        "#define LEVEL 2\n\nvoid f(void)\n{\n#if LEVEL > 1 && defined(LEVEL)\n    wait;\n#elif 1\n    end;\n#else\n    not even code;\n#endif\n#ifdef MISSING\n    end;\n#endif\n#ifndef MISSING\n    wait;\n#endif\n}\n",
        "void f(void)\n{\n    wait;\n    wait;\n}\n",
        HEAD,
    );
}

#[test]
fn natives_in_expressions_are_called_into_a_hidden_local_first() {
    same(
        "void f(void)\n{\n    int a;\n\n    if (get_flag(5) == 1) {\n        a = get_counter(2) + 1;\n    }\n}\n",
        "void f(void)\n{\n    int a;\n    int __value_0;\n    int __value_1;\n\n    get_flag(5, &__value_0);\n    if (__value_0 == 1) {\n        get_counter(2, &__value_1);\n        a = __value_1 + 1;\n    }\n}\n",
        EVENT,
    );
}

#[test]
fn a_native_in_a_loop_condition_runs_every_iteration() {
    same(
        "void f(void)\n{\n    while (get_flag(5)) {\n        wait;\n    }\n}\n",
        "void f(void)\n{\n    int __value_0;\n\n    while (1) {\n        get_flag(5, &__value_0);\n        if (!__value_0) {\n            break;\n        }\n        wait;\n    }\n}\n",
        EVENT,
    );
}

#[test]
fn natives_run_only_when_c_evaluates_them() {
    same(
        "int f(void)\n{\n    int a;\n    int x;\n\n    x = a && get_flag(5);\n    if (a || get_counter(2) > 1) {\n        wait;\n    }\n    return a ? get_flag(6) : x;\n}\n",
        "int f(void)\n{\n    int a;\n    int x;\n    int __value_0;\n    int __value_1;\n    int __value_2;\n    int __value_3;\n    int __value_4;\n    int __value_5;\n\n    __value_0 = !!a;\n    if (__value_0) {\n        get_flag(5, &__value_1);\n        __value_0 = !!__value_1;\n    }\n    x = !!__value_0;\n    __value_2 = !!a;\n    if (!__value_2) {\n        get_counter(2, &__value_3);\n        __value_2 = !!(__value_3 > 1);\n    }\n    if (__value_2) {\n        wait;\n    }\n    if (a) {\n        get_flag(6, &__value_5);\n        __value_4 = __value_5;\n    } else {\n        __value_4 = x;\n    }\n    return __value_4;\n}\n",
        EVENT,
    );
}

#[test]
fn natives_take_int_and_float_arguments_as_they_are() {
    // the engine converts what it reads (GetStackInt, GetStackFloat), so 1 goes as the int 1
    let text = "void f(void)\n{\n    set_bgm_master_volume(1);\n}\n";
    same(text, "void f(void)\n{\n    set_bgm_master_volume((any)1);\n}\n", EVENT);
    assert_ne!(built(text, EVENT).unwrap(), built(&text.replace("(1)", "(1.0)"), EVENT).unwrap());
}

#[test]
fn zero_stored_in_a_float_stays_the_int_zero() {
    // its bits are 0.0's, and a float read makes the slot a float again
    same(
        "void f(void)\n{\n    float x;\n\n    x = 0;\n}\n",
        "void f(void)\n{\n    float x;\n\n    x = (any)0;\n}\n",
        HEAD,
    );
}

#[test]
fn any_keeps_a_native_result_as_the_native_wrote_it() {
    // the native writes straight into the variable, as x = native(); does for a float
    same(
        "void f(void)\n{\n    int x;\n\n    x = (any)get_battle_bgm_volume();\n}\n",
        "void f(void)\n{\n    int x;\n\n    _GET_BTL_BGM_VOL(&x);\n}\n",
        EVENT,
    );
    // without it C converts the float result for the int
    assert_ne!(
        built("void f(void)\n{\n    int x;\n\n    x = get_battle_bgm_volume();\n}\n", EVENT).unwrap(),
        built("void f(void)\n{\n    int x;\n\n    _GET_BTL_BGM_VOL(&x);\n}\n", EVENT).unwrap()
    );
}

// Features with code of their own are run.

#[test]
fn values_convert_as_c_converts_them() {
    // an int passed for a float parameter is made a float, a float stored in an int truncated
    let passed = "float half(float x)\n{\n    return x / 2;\n}\n\nint f(int v)\n{\n    return half(v) * 10;\n}\n";
    assert_eq!(result(passed, &[3]), int(15));
    let stored = "int f(int v)\n{\n    int x;\n\n    x = v / 2.0;\n    return x * 2;\n}\n";
    assert_eq!(result(stored, &[3]), int(2));
    // (any) stores the value as it is: x holds 1.5, and x * 2 is the float 3.0
    assert_eq!(result(&stored.replace("x = v / 2.0;", "x = (any)(v / 2.0);"), &[3]), Value::Float(3.0));
}

#[test]
fn logical_values_are_0_or_1_and_any_keeps_the_vms() {
    let text = "int f(int v)\n{\n    int x;\n\n    x = v && 7;\n    return x;\n}\n";
    assert_eq!(result(text, &[1]), int(1));
    // the VM's && leaves the deciding operand
    assert_eq!(result(&text.replace("x = v && 7;", "x = (any)(v && 7);"), &[1]), int(7));
}

#[test]
fn switch_compares_in_order_falls_through_and_breaks() {
    let text = "int f(int v)\n{\n    int out = 0;\n\n    switch (v) {\n    case 1:\n        out += 10;\n    case 2:\n        out += 20;\n        break;\n    case 3:\n    case 4:\n        out = 34;\n        break;\n    default:\n        out = -1;\n    }\n    return out;\n}\n";
    let found: Vec<Value> = [1, 2, 3, 4, 9].iter().map(|value| result(text, &[*value])).collect();
    assert_eq!(found, [int(30), int(20), int(34), int(34), int(-1)]);
}

#[test]
fn continue_in_a_switch_continues_the_loop() {
    let text = "int f(void)\n{\n    int total = 0;\n\n    for (int i = 0; i < 5; i++) {\n        switch (i) {\n        case 2:\n            continue;\n        }\n        total += i;\n    }\n    return total;\n}\n";
    assert_eq!(result(text, &[]), int(1 + 3 + 4));
}

#[test]
fn ternary_xor_shifts_and_not() {
    assert_eq!(result("int f(int a)\n{\n    return a > 2 ? 100 : 200;\n}\n", &[3]), int(100));
    assert_eq!(result("int f(int a)\n{\n    return a > 2 ? 100 : a ? 150 : 200;\n}\n", &[1]), int(150));
    assert_eq!(result("int f(int a)\n{\n    return a ^ 6;\n}\n", &[5]), int(3));
    assert_eq!(result("int f(int a)\n{\n    return (1 << a) | (64 >> 2);\n}\n", &[3]), int(8 | 16));
    assert_eq!(result("int f(int a)\n{\n    int n = 2;\n\n    return (a << n) + (a >> n);\n}\n", &[20]), int(80 + 5));
    assert_eq!(result("int f(int a)\n{\n    return ~a;\n}\n", &[5]), int(-6));
}

#[test]
fn postfix_steps_return_the_old_value() {
    let text = "int f(void)\n{\n    int i = 5;\n    int j;\n\n    j = i++;\n    return j * 100 + i;\n}\n";
    assert_eq!(result(text, &[]), int(506));
    assert_eq!(result("int f(void)\n{\n    int i = 5;\n\n    return --i;\n}\n", &[]), int(4));
}

#[test]
fn arrays_pass_as_pointers_and_pointers_dereference() {
    let text = "void fill(int *out, int count)\n{\n    for (int i = 0; i < count; i++) {\n        out[i] = i * i;\n    }\n    *out = 7;\n}\n\nint f(void)\n{\n    int squares[4];\n\n    fill(squares, 4);\n    return squares[0] * 100 + squares[3];\n}\n";
    assert_eq!(common::run(&format!("{HEAD}{text}"), "f", &[]).unwrap(), int(709));
}

#[test]
fn pointer_locals_point_at_variables() {
    let text = "int f(void)\n{\n    int x = 1;\n    int *p = &x;\n\n    *p = 42;\n    p[0] += 1;\n    return x;\n}\n";
    assert_eq!(result(text, &[]), int(43));
}

#[test]
fn string_variables_hold_strings() {
    let text =
        "int f(void)\n{\n    string s = \"hello\";\n    string t;\n\n    t = s;\n    print(t);\n    return 1;\n}\n";
    assert_eq!(result(text, &[]), int(1));
}

// Unsupported C is refused with the line it is on.

#[test]
fn errors_name_the_line() {
    fails("void f(void)\n{\n    wait;\n    nope = 1;\n}\n", "line 9: unknown variable nope");
    fails("int g[2];\nint f(void)\n{\n    return g + 1 == g;\n}\n", "line 9: comparing two pointers is not supported");
    fails("int h;\nint g = h;\nvoid f(void)\n{\n}\n", "the value of global g must be a constant expression, not h");
    fails(
        "#define TWICE(x) ((x) * 2)\nvoid f(void)\n{\n    TWICE(1, 2);\n}\n",
        "line 9: macro TWICE takes 1 argument, given 2",
    );
    fails("void f(void)\n{\n#if 1\n}\n", "#if without #endif");
    fails("void f(void)\n{\n    int a;\n    float a;\n}\n", "a is declared twice in one block");
    fails("void f(void)\n{\n    break;\n}\n", "break outside a loop or switch");
}

#[test]
fn a_librarys_defines_reach_the_script_that_includes_it() {
    let script =
        parse::parse(&format!("{HEAD}#include \"lib/sizes.c\"\n\nint f(void)\n{{\n    return twice(SIZE);\n}}\n"))
            .unwrap();
    let found = texts(&[("lib/sizes.c", "#define SIZE 21\n\nint twice(int value)\n{\n    return value * 2;\n}\n")]);
    let program = library::expand(script, &found, sdk_texts()).unwrap();
    let table = vm::table_of(&program, tables()).unwrap();
    assert_eq!(run_function(&table, "f", vec![], vec![int(0); 4]).unwrap(), int(42));
}

#[test]
fn every_header_a_script_includes_is_read() {
    let mut headers = sdk_texts().clone();
    headers.insert("extra.h".into(), "enum extra {\n    EXTRA_VALUE = 9,\n};\n".into());
    let script = parse::parse(
        "#include <event.h>\n#include <extra.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\nint f(void)\n{\n    return EXTRA_VALUE + PAD_CROSS;\n}\n",
    )
    .unwrap();
    let program = library::expand(script, &common::no_texts(), &headers).unwrap();
    let table = vm::table_of(&program, tables()).unwrap();
    assert_eq!(run_function(&table, "f", vec![], vec![]).unwrap(), int(9 + 64));
}

#[test]
fn a_comment_after_an_include_is_a_space() {
    let text = "int f(void)\n{\n    return PAD_CROSS;\n}\n";
    for head in [
        "#include <event.h> // natives\n\nPROGRAM(100, f);\nMAIN(f);\n\n",
        "#include <event.h> /* natives */\n\nPROGRAM(100, f);\nMAIN(f);\n\n",
        "#include <event.h> /* the natives,\n                     the constants */\nPROGRAM(100, f);\nMAIN(f);\n\n",
    ] {
        assert_eq!(built(text, head).unwrap(), built(text, EVENT).unwrap(), "{head}");
    }
}

#[test]
fn libraries_include_libraries_once_and_shared_ones_come_from_the_sdk() {
    let folder = TempDir::new();
    let root = folder.path();
    write(
        &root.join("lib/a.c"),
        "#include \"lib/b.c\"\n#include \"modkit/input.c\"\n\nint a(void)\n{\n    return b() + 1;\n}\n",
    );
    write(&root.join("lib/b.c"), "int b(void)\n{\n    return 40;\n}\n");
    let script = parse::parse(
        "#include <event.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\n#include \"lib/a.c\"\n#include \"lib/b.c\"\n\nint f(void)\n{\n    modkit_wait(2);\n    return a() + 1;\n}\n",
    )
    .unwrap();
    let found = library::read_includes(&script, &root.join("script.c"), None, &sdk_dir(), &[]).unwrap();
    let mut paths: Vec<&String> = found.keys().collect();
    paths.sort();
    assert_eq!(paths, ["lib/a.c", "lib/b.c", "modkit/input.c"]);
    let program = library::expand(script, &found, sdk_texts()).unwrap();
    let functions = program["functions"].as_array().unwrap();
    assert_eq!(functions.iter().filter(|function| function["name"] == "b").count(), 1);
    let table = vm::table_of(&program, tables()).unwrap();
    assert_eq!(run_function(&table, "f", vec![], vec![]).unwrap(), int(42));
}

#[test]
fn a_compound_assignment_works_out_its_target_once() {
    let text = "int f(void)\n{\n    int a[3] = {10, 20, 30};\n    int i = 0;\n\n    a[i++] += 5;\n    a[i++]++;\n    return a[0] * 10000 + a[1] * 100 + i;\n}\n";
    assert_eq!(result(text, &[]), int(15 * 10000 + 21 * 100 + 2));
    let floats = "float f(void)\n{\n    float v[2] = {1.5, 2.0};\n\n    v[1] *= 2.5;\n    return v[1];\n}\n";
    assert_eq!(result(floats, &[]), Value::Float(5.0));
}

#[test]
fn dividing_by_zero_stops_the_test_vm_as_it_stops_the_game() {
    for text in [
        "int f(int a)\n{\n    return 7 / a;\n}\n",
        "float f(int a)\n{\n    return 7.0 / a;\n}\n",
        "int f(int a)\n{\n    return 7 % a;\n}\n",
    ] {
        match common::run(&format!("{HEAD}{text}"), "f", &[int(0)]) {
            Err(error) => assert!(error.to_string().contains("division by zero"), "{error}"),
            Ok(value) => panic!("divided by zero: {text} gave {value:?}"),
        }
    }
}

// Standard C's forms meet the scripts' own: natives, the SDK's kinds, hoisted results.

#[test]
fn natives_work_in_cs_new_forms() {
    let text = "#define READ(n) get_counter(n)\nstatic int calls;\nint lives = 3;\n\nint f(void)\n{\n    static int seen = 2;\n    unsigned char low = READ(3) + 300;\n    flag which = 5;\n    int i;\n\n    for (i = 0, calls++; i < 2 && get_flag(which); i++) {\n        goto out;\n    }\nout:\n    seen++;\n    return low + seen * 1000 + lives * 100 + calls;\n}\n";
    // natives do nothing in the test VM, so their results are 0
    let table = common::functions(&format!("{EVENT}{text}")).unwrap();
    let found = common::run_function(&table, "f", vec![], vec![int(0); 8]).unwrap();
    assert_eq!(found, int(44 + 3000 + 300 + 1));
}

#[test]
fn hidden_globals_and_the_runtime_are_added_only_when_used() {
    let global_num = |bytes: &[u8]| i32::from_le_bytes(bytes[0x18..0x1c].try_into().unwrap());
    let plain = built("int g;\n\nint f(void)\n{\n    return g / 2;\n}\n", HEAD).unwrap();
    let initialised = built("int g = 4;\n\nint f(void)\n{\n    return g / 2;\n}\n", HEAD).unwrap();
    let unsigned = built("unsigned g;\n\nint f(void)\n{\n    return g / 2;\n}\n", HEAD).unwrap();
    assert!(initialised.len() > plain.len() && unsigned.len() > plain.len());
    // the flag that sets the values once is a global of its own
    assert_eq!((global_num(&plain), global_num(&initialised), global_num(&unsigned)), (1, 2, 1));
}
