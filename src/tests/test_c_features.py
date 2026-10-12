from c2s import compile, natives, parse
from c2s import vm

HEAD = "#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\n"
EVENT = "#include <event.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\n"


def built(text, head=HEAD):
    from c2s import library, sdk
    program = library.expand(parse.parse(head + text), {}, sdk.load_texts())
    return compile.compile_program(program, natives.load())


def same(text, plain, head=HEAD):
    assert built(text, head) == built(plain, head), text


def result(text, args=()):
    return vm.run(HEAD + text, "f", args)


def fails(text, message):
    try:
        built(text)
    except ValueError as error:
        assert message in str(error), str(error)
        return
    raise AssertionError("compiled: " + text)


# Features that are a different way to write code the compiler already took compile to the same
# bytes as that code.

def test_initializers_and_declarations_anywhere():
    same("void f(void)\n{\n    int a = 1;\n    wait;\n    float b = 2.0;\n    int c[3] = {4, 5};\n}\n",
         "void f(void)\n{\n    int a;\n    float b;\n    int c[3];\n\n    a = 1;\n    wait;\n    b = 2.0;\n"
         "    c[0] = 4;\n    c[1] = 5;\n}\n")
    same("void f(void)\n{\n    int a[] = {7, 8};\n\n    a[1] = 0;\n}\n",
         "void f(void)\n{\n    int a[2];\n\n    a[0] = 7;\n    a[1] = 8;\n    a[1] = 0;\n}\n")


def test_for_loops_are_while_loops_that_step_before_continue():
    same("void f(void)\n{\n    for (int i = 0; i < 3; i++) {\n        if (i == 1) {\n            continue;\n        }\n        wait;\n    }\n}\n",
         "void f(void)\n{\n    int i;\n\n    i = 0;\n    while (i < 3) {\n        if (i == 1) {\n            i = i + 1;\n"
         "            continue;\n        }\n        wait;\n        i = i + 1;\n    }\n}\n")


def test_do_loops_test_after_the_body():
    same("void f(void)\n{\n    int i;\n\n    do {\n        wait;\n    } while (i);\n}\n",
         "void f(void)\n{\n    int i;\n\n    while (1) {\n        wait;\n        if (!i) {\n            break;\n        }\n    }\n}\n")


def test_compound_assignment_steps_and_braceless_bodies():
    same("void f(void)\n{\n    int i;\n\n    i += 2;\n    i *= 3;\n    ++i;\n    i--;\n    if (i) wait;\n    else end;\n}\n",
         "void f(void)\n{\n    int i;\n\n    i = i + 2;\n    i = i * 3;\n    i = i + 1;\n    i = i - 1;\n"
         "    if (i) {\n        wait;\n    } else {\n        end;\n    }\n}\n")


def test_literals_blocks_and_empty_statements():
    same("void f(void)\n{\n    int c;\n    float x;\n\n    c = 'A' + '\\n';\n    x = 2.5f;\n    { ; }\n}\n",
         "void f(void)\n{\n    int c;\n    float x;\n\n    c = 65 + 10;\n    x = 2.5;\n}\n")


def test_preprocessor_conditions_choose_code():
    same("#define LEVEL 2\n\nvoid f(void)\n{\n#if LEVEL > 1 && defined(LEVEL)\n    wait;\n#elif 1\n    end;\n#else\n"
         "    not even code;\n#endif\n#ifdef MISSING\n    end;\n#endif\n#ifndef MISSING\n    wait;\n#endif\n}\n",
         "void f(void)\n{\n    wait;\n    wait;\n}\n")


def test_natives_in_expressions_are_called_into_a_hidden_local_first():
    same("void f(void)\n{\n    int a;\n\n    if (get_flag(5) == 1) {\n        a = get_counter(2) + 1;\n    }\n}\n",
         "void f(void)\n{\n    int a;\n    int __value_0;\n    int __value_1;\n\n    get_flag(5, &__value_0);\n"
         "    if (__value_0 == 1) {\n        get_counter(2, &__value_1);\n        a = __value_1 + 1;\n    }\n}\n", EVENT)


def test_a_native_in_a_loop_condition_runs_every_iteration():
    same("void f(void)\n{\n    while (get_flag(5)) {\n        wait;\n    }\n}\n",
         "void f(void)\n{\n    int __value_0;\n\n    while (1) {\n        get_flag(5, &__value_0);\n"
         "        if (!__value_0) {\n            break;\n        }\n        wait;\n    }\n}\n", EVENT)


# Features with code of their own are run.

def test_switch_compares_in_order_falls_through_and_breaks():
    text = ("int f(int v)\n{\n    int out = 0;\n\n    switch (v) {\n    case 1:\n        out += 10;\n    case 2:\n"
            "        out += 20;\n        break;\n    case 3:\n    case 4:\n        out = 34;\n        break;\n"
            "    default:\n        out = -1;\n    }\n    return out;\n}\n")
    assert [result(text, [value]) for value in (1, 2, 3, 4, 9)] == [30, 20, 34, 34, -1]


def test_continue_in_a_switch_continues_the_loop():
    text = ("int f(void)\n{\n    int total = 0;\n\n    for (int i = 0; i < 5; i++) {\n        switch (i) {\n"
            "        case 2:\n            continue;\n        }\n        total += i;\n    }\n    return total;\n}\n")
    assert result(text) == 0 + 1 + 3 + 4


def test_ternary_xor_shifts_and_not():
    assert result("int f(int a)\n{\n    return a > 2 ? 100 : 200;\n}\n", [3]) == 100
    assert result("int f(int a)\n{\n    return a > 2 ? 100 : a ? 150 : 200;\n}\n", [1]) == 150
    assert result("int f(int a)\n{\n    return a ^ 6;\n}\n", [5]) == 3
    assert result("int f(int a)\n{\n    return (1 << a) | (64 >> 2);\n}\n", [3]) == 8 | 16
    assert result("int f(int a)\n{\n    int n = 2;\n\n    return (a << n) + (a >> n);\n}\n", [20]) == 80 + 5
    assert result("int f(int a)\n{\n    return ~a;\n}\n", [5]) == -6


def test_postfix_steps_return_the_old_value():
    text = "int f(void)\n{\n    int i = 5;\n    int j;\n\n    j = i++;\n    return j * 100 + i;\n}\n"
    assert result(text) == 506
    assert result("int f(void)\n{\n    int i = 5;\n\n    return --i;\n}\n") == 4


def test_arrays_pass_as_pointers_and_pointers_dereference():
    text = ("void fill(int *out, int count)\n{\n    for (int i = 0; i < count; i++) {\n        out[i] = i * i;\n"
            "    }\n    *out = 7;\n}\n\nint f(void)\n{\n    int squares[4];\n\n    fill(squares, 4);\n"
            "    return squares[0] * 100 + squares[3];\n}\n")
    assert vm.run(HEAD.replace("MAIN(f)", "MAIN(f)") + text, "f") == 709


def test_pointer_locals_point_at_variables():
    text = "int f(void)\n{\n    int x = 1;\n    int *p = &x;\n\n    *p = 42;\n    p[0] += 1;\n    return x;\n}\n"
    assert result(text) == 43


def test_string_variables_hold_strings():
    text = 'int f(void)\n{\n    string s = "hello";\n    string t;\n\n    t = s;\n    print(t);\n    return 1;\n}\n'
    assert result(text) == 1


# Unsupported C is refused with the line it is on.

def test_errors_name_the_line():
    fails("void f(void)\n{\n    wait;\n    nope = 1;\n}\n", "line 9: unknown variable nope")
    fails("int g[2];\n", "global arrays are not supported")
    fails("int g = 1;\n", "globals start at 0")
    fails("#define TWICE(x) ((x) * 2)\n", "macros with parameters")
    fails("void f(void)\n{\n#if 1\n}\n", "#if without #endif")
    fails("void f(void)\n{\n    int a;\n    float a;\n}\n", "declared again with another type")
    fails("void f(void)\n{\n    break;\n}\n", "break outside a loop or switch")


def test_a_librarys_defines_reach_the_script_that_includes_it():
    from c2s import library, sdk
    script = parse.parse(HEAD + '#include "lib/sizes.c"\n\nint f(void)\n{\n    return twice(SIZE);\n}\n')
    texts = {"lib/sizes.c": "#define SIZE 21\n\nint twice(int value)\n{\n    return value * 2;\n}\n"}
    program = library.expand(script, texts, sdk.load_texts())
    assert vm.run_function(vm.table_of(program), "f", [], [("int", 0)] * 4)[1] == 42


def test_every_header_a_script_includes_is_read():
    from c2s import library, sdk
    headers = dict(sdk.load_texts(), **{"extra.h": "enum extra {\n    EXTRA_VALUE = 9,\n};\n"})
    script = parse.parse("#include <event.h>\n#include <extra.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\n"
                         "int f(void)\n{\n    return EXTRA_VALUE + PAD_CROSS;\n}\n")
    program = library.expand(script, {}, headers)
    assert vm.run_function(vm.table_of(program), "f", [], []) [1] == 9 + 64


def test_libraries_include_libraries_once_and_shared_ones_come_from_the_sdk():
    import tempfile
    from pathlib import Path
    from c2s import library, sdk
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "lib").mkdir()
        (root / "lib/a.c").write_text('#include "lib/b.c"\n#include "modkit/input.c"\n\nint a(void)\n{\n    return b() + 1;\n}\n',
                                      encoding="utf-8")
        (root / "lib/b.c").write_text("int b(void)\n{\n    return 40;\n}\n", encoding="utf-8")
        script = parse.parse('#include <event.h>\n\nPROGRAM(100, f);\nMAIN(f);\n\n#include "lib/a.c"\n#include "lib/b.c"\n\n'
                             "int f(void)\n{\n    modkit_wait(2);\n    return a() + 1;\n}\n")
        texts = library.read_includes(script, root / "script.c")
        assert sorted(texts) == ["lib/a.c", "lib/b.c", "modkit/input.c"]
        program = library.expand(script, texts, sdk.load_texts())
        assert [function["name"] for function in program["functions"]].count("b") == 1
        assert vm.run_function(vm.table_of(program), "f", [], [])[1] == 42
