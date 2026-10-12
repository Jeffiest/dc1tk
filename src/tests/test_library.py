import json
import tempfile
from pathlib import Path

from c2s import compile, decompile, definitions, library, names, natives, parse, sdk, source, stb

HELPERS = """
void wait_frames(int count)
{
    while (count > 0) {
        wait;
        count = count - 1;
    }
}

int twice(int value)
{
    wait_frames(value);
    return value * 2;
}
"""

SCRIPT = """#pragma natives event

PROGRAM(100, talk);
MAIN(talk);
%s
void talk(void)
{
    int done;

    done = twice(%d);
    _GET_FLAG(12, &done);
}
"""


def script_bytes(own):
    return compile.compile_program(parse.parse(SCRIPT % (HELPERS, own)), natives.load())


def corpus(folder):
    root = Path(folder)
    for number in range(3):
        (root / "map" / ("m%d.stb" % number)).parent.mkdir(parents=True, exist_ok=True)
        (root / "map" / ("m%d.stb" % number)).write_bytes(script_bytes(number))
    (root / "map/solo.stb").write_bytes(compile.compile_program(
        parse.parse((SCRIPT % ("", 0)).replace("twice(0)", "1")), natives.load()))
    return root, ["map/m0.stb", "map/m1.stb", "map/m2.stb", "map/solo.stb"]


def test_shared_functions_become_one_library():
    with tempfile.TemporaryDirectory() as folder:
        root, paths = corpus(folder)
        model = library.model_for(root, paths, natives.load(), definitions.empty())
        texts = library.library_texts(model)
        assert list(texts) == ["lib/map/map_1.c"]
        assert "void map_1_1(int a0)" in texts["lib/map/map_1.c"]
        assert "map_1_1(a0);" in texts["lib/map/map_1.c"]
        for path in paths:
            data = (root / path).read_bytes()
            program = decompile.decompile(stb.read(data), "event", natives.load())
            text = source.text(library.script_program(program, path, model))
            if path != "map/solo.stb":
                assert '#include "lib/map/map_1.c"' in text
                assert "void program_100(void)" in text
                assert "v0 = map_1_2(" in text
            parsed = parse.parse(text)
            assert compile.compile_program(library.expand(parsed, texts), natives.load()) == data


def test_name_table_renames_everywhere():
    with tempfile.TemporaryDirectory() as folder:
        root, paths = corpus(folder)
        model = library.model_for(root, paths, natives.load(), definitions.empty())
        fingerprint = [key for key, name in model["class_names"].items() if name == "map_1_1"][0]
        table = Path(folder) / "names"
        table.mkdir()
        entry = {"name": "wait_frames", "params": ["frames"]}
        (table / "event.json").write_text(json.dumps({"functions": {fingerprint: entry}}))
        model = library.model_for(root, paths, natives.load(), dict(definitions.empty(), names=names.load(table)))
        texts = library.library_texts(model)
        text = texts["lib/map/map_1.c"]
        assert "void wait_frames(int frames)" in text
        assert "while (frames > 0) {" in text
        assert "wait_frames(a0);" in text
        data = (root / "map/m1.stb").read_bytes()
        program = decompile.decompile(stb.read(data), "event", natives.load())
        parsed = parse.parse(source.text(library.script_program(program, "map/m1.stb", model)))
        assert compile.compile_program(library.expand(parsed, texts), natives.load()) == data


def test_clashing_variable_names_are_numbered():
    function = parse.parse("MAIN(f);\nvoid f(int a0, int a1)\n{\n    g0 = a0;\n}\n")["functions"][0]
    assert [param["name"] for param in names.with_variables(function, {"params": ["x", "x"]})["params"]] == ["x", "x_2"]
    assert names.with_variables(function, {"params": ["g0"]})["params"][0]["name"] == "g0_2"
    assert names.with_variables(function, {"params": ["x"]})["params"][0]["name"] == "x"


def test_padding_of_overrides_an_included_function():
    program = parse.parse('#pragma natives event\nPROGRAM(100, talk);\nMAIN(talk);\n#include "lib.c"\nvoid talk(void)\n{\n}\n'
                          "#pragma padding_of helper 01020304\n")
    texts = {"lib.c": "#pragma natives event\nvoid helper(void)\n{\n    print(\"abc\");\n}\n"}
    expanded = library.expand(program, texts)
    assert [function["name"] for function in expanded["functions"]] == ["helper", "talk"]
    assert expanded["functions"][0]["padding"] == "01020304"
    script = stb.read(compile.compile_program(expanded, natives.load()))
    assert script["regions"][2]["padding"] == "01020304"


def test_includes_are_found_in_parent_folders():
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "lib").mkdir()
        (root / "lib/a.c").write_text("#pragma natives event\n")
        (root / "map/s").mkdir(parents=True)
        program = parse.parse('#include "lib/a.c"\nMAIN(f);\n')
        assert library.read_includes(program, root / "map/s/x.c") == {"lib/a.c": "#pragma natives event\n"}


def test_native_calls_are_resolved_after_includes():
    program = parse.parse("#pragma natives event\nMAIN(f);\nvoid helper(void)\n{\n}\nvoid f(void)\n{\n"
                          "    helper();\n    _SET_FLAG(1, 1);\n}\n")
    body = sdk.resolve(program, natives.load())["functions"][1]["body"]
    assert body[0]["kind"] == "expr" and body[1]["kind"] == "native"
    for broken in ("MAIN(f);\nvoid _f(void)\n{\n}\n", "#pragma natives event\nMAIN(f);\nvoid f(void)\n{\n    int x;\n\n    x = _GET_FLAG(1) + 1;\n}\n"):
        try:
            sdk.resolve(parse.parse(broken), natives.load())
        except ValueError:
            continue
        raise AssertionError("accepted " + broken)


def test_libraries_json_moves_and_documents_a_library():
    with tempfile.TemporaryDirectory() as folder:
        root, paths = corpus(folder)
        model = library.model_for(root, paths, natives.load(), definitions.empty())
        identity = next(iter(model["libraries"].values()))["fingerprint"]
        defs = dict(definitions.empty(), libraries={identity: {"path": "lib/town/waiting.c", "doc": "Waiting helpers."}})
        model = library.model_for(root, paths, natives.load(), defs)
        texts = library.library_texts(model)
        assert list(texts) == ["lib/town/waiting.c"]
        assert texts["lib/town/waiting.c"].startswith("// Waiting helpers.\n\n")
        data = (root / "map/m1.stb").read_bytes()
        program = decompile.decompile(stb.read(data), "event", natives.load())
        text = source.text(library.script_program(program, "map/m1.stb", model))
        assert '#include "lib/town/waiting.c"' in text
        assert compile.compile_program(library.expand(parse.parse(text), texts), natives.load()) == data


GLOBAL_SCRIPT = """#pragma natives event

int counter;

PROGRAM(100, talk);
MAIN(talk);

void bump(void)
{
    counter = counter + 1;
}

void talk(void)
{
    bump();
    _GET_FLAG(%d, &counter);
}
"""


def test_libraries_json_names_the_globals_a_library_uses():
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        paths = []
        for number in range(3):
            path = "map/g%d.stb" % number
            (root / path).parent.mkdir(parents=True, exist_ok=True)
            (root / path).write_bytes(compile.compile_program(parse.parse(GLOBAL_SCRIPT % (number + 10)), natives.load()))
            paths.append(path)
        model = library.model_for(root, paths, natives.load(), definitions.empty())
        identity = next(iter(model["libraries"].values()))["fingerprint"]
        defs = dict(definitions.empty(), libraries={identity: {"globals": {"g0": "visits"}}})
        model = library.model_for(root, paths, natives.load(), defs)
        texts = library.library_texts(model)
        assert "visits = visits + 1;" in next(iter(texts.values()))
        for path in paths:
            data = (root / path).read_bytes()
            program = decompile.decompile(stb.read(data), "event", natives.load())
            text = source.text(library.script_program(program, path, model))
            assert "int visits;" in text
            assert compile.compile_program(library.expand(parse.parse(text), texts), natives.load()) == data
