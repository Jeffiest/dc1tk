from c2s import asm, compile, decompile, definitions, natives, parse, sdk, source, stb
from tests import build

EVENT = """#pragma natives event

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
"""

NAMED = """#pragma natives event

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
"""

MONSTER = """#pragma natives monster

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
"""


def build_text(text):
    tables = natives.load()
    return compile.compile_program(parse.parse(text), tables)


def decompiled(data, table):
    return source.text(decompile.decompile(stb.read(data), table, natives.load()))


# Parsed statements carry their source line, for error messages; the decompiler's have none.
def without_lines(node):
    if isinstance(node, list):
        return [without_lines(item) for item in node]
    if isinstance(node, dict):
        return {key: without_lines(value) for key, value in node.items() if key not in ("line", "start")}
    return node


def test_source_round_trips_through_bytes():
    data = build_text(EVENT)
    assert decompiled(data, "event") == EVENT
    resolved = sdk.resolve(parse.parse(EVENT), natives.load())
    tree = decompile.decompile(stb.read(data), "event", natives.load())
    assert sorted(tree.pop("fingerprints")) == ["f_0000", "f_04d4"]
    assert sorted(tree.pop("loose_fingerprints")) == ["f_0000", "f_04d4"]
    assert without_lines(resolved) == without_lines(tree)


def test_names_are_free_to_choose():
    assert build_text(NAMED) == build_text(EVENT)


def test_grouped_and_unregistered_natives():
    data = build_text(MONSTER)
    assert decompiled(data, "monster") == MONSTER
    text = asm.disassemble(stb.read(data), "monster", natives.load())
    assert "    native _GET_DIST\n    push 15\n    add\n" in text
    assert "    native _SET_DAMAGE_SCORE\n    push 12\n    add\n" in text
    assert "    native _SET_MOS\n" in text
    assert "    native 190\n" in text


def test_negation_is_an_instruction():
    text = asm.disassemble(stb.read(build_text(EVENT)), "event", natives.load())
    assert "    push 2.0\n    neg\n" in text


def test_labels_follow_allocation_order():
    text = asm.disassemble(stb.read(build_text(EVENT)), "event", natives.load())
    numbers = [int(line.split()[1]) for line in text.split("\n") if line.startswith("    nop ")]
    assert numbers == [0, 1, 2, 4, 5, 3, 6, 8, 9, 7]


def test_unused_strings_and_padding_pragmas():
    text = EVENT.replace("void f_0000(void)", '#pragma strings "old" "older"\n#pragma padding 01020304\nvoid f_0000(void)')
    data = build_text(text)
    main = stb.read(data)["regions"][2]
    assert main["strings"] == ["old", "older", "hello"]
    assert main["padding"] == "01020304"
    again = decompiled(data, "event")
    assert '#pragma strings "old" "older"\nvoid f_0000(void)' in again
    assert again.endswith("\n#pragma padding_of f_0000 01020304\n")
    assert build_text(again) == data


def test_whole_file_assembly_fallback():
    data = build.sample_script()
    text = decompiled(data, "event")
    assert text.count("\nasm f_") == 2
    assert build_text(text) == data


def test_errors_name_the_line():
    for broken, message in (("void f_0000(void)\n{\n    v0 = ;\n}\n", "line 3"),
                            ("MAIN(f_0000);\nvoid f_0000(void)\n{\n    wait\n}\n", "line 5"),
                            ('MAIN(f_0000);\nvoid f_0000(void)\n{\n    print("a);\n}\n', "line 4")):
        try:
            parse.parse(broken)
        except ValueError as error:
            assert message in str(error), str(error)
        else:
            raise AssertionError("parsed " + broken)


def test_unknown_names_fail_to_compile():
    for broken in (EVENT.replace("_GET_FLAG", "_NO_SUCH_NATIVE"), EVENT.replace("g0 = g0 + 1", "g9 = 1"),
                   EVENT.replace("f_04d4(&v2[0], (float)v0)", "f_04d4(1)")):
        try:
            build_text(broken)
        except ValueError:
            continue
        raise AssertionError("compiled a broken program")


def test_fingerprints_ignore_names_and_position():
    first = decompile.decompile(stb.read(build_text(EVENT)), "event", natives.load())["fingerprints"]
    second = decompile.decompile(stb.read(build_text(NAMED)), "event", natives.load())["fingerprints"]
    assert sorted(first.values()) == sorted(second.values())
    moved = build_text(EVENT.replace("int g0;\n", "int g0;\n").replace("    v0 = 3;\n", "    v0 = 4;\n"))
    changed = decompile.decompile(stb.read(moved), "event", natives.load())["fingerprints"]
    assert changed["f_0000"] != first["f_0000"] and changed["f_04d4"] == first["f_04d4"]


def test_editor_flags_point_clang_at_the_sdk_the_sets_and_the_tree():
    lines = source.editor_flags(sdk.SDK_DIR, [str(definitions.DEFAULT_SET)]).splitlines()
    assert lines[:3] == ["-xc", "-std=c2x", "-m32"]
    assert "-I" + sdk.SDK_DIR.resolve().as_posix() in lines
    assert "-I" + (definitions.DEFAULT_SET / "include").resolve().as_posix() in lines
    assert lines[-2:] == ["-Iinclude", "-I."]


def test_docs_read_back_as_written():
    doc = "Plays a back flip, pushing backwards while the flip is mid-motion; " * 3 + "\nA second paragraph."
    lines = source.doc_lines(doc)
    comments = [(number, line[2:].strip()) for number, line in enumerate(lines, 1)]
    assert parse.comment_block(comments, len(lines) + 1) == doc.replace("; \n", ";\n").strip()


def test_a_program_naming_no_function_says_so():
    try:
        build_text(EVENT.replace("PROGRAM(200, f_04d4);", "PROGRAM(200, renamed_away);"))
    except ValueError as error:
        assert str(error) == "PROGRAM(200, renamed_away): no function renamed_away"
    else:
        raise AssertionError("compiled a broken program")



def test_a_string_pool_file_round_trips_without_pragmas_per_function():
    text = EVENT.replace("#pragma natives event\n", "#pragma natives event\n#pragma string_pool\n", 1)
    data = build_text(text)
    again = decompiled(data, "event")
    assert "#pragma string_pool\n" in again
    assert "#pragma strings" not in again and "#pragma padding_of" not in again
    assert build_text(again) == data
