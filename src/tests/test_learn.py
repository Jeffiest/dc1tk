import json
import tempfile
from pathlib import Path

from c2s import compile, decompile, definitions, learn, names, natives, parse, sdk, source, stb
from tests import test_source as examples

DOCUMENTED = examples.NAMED.replace("void start(void)", "// Greets the visitor and counts the visit.\nvoid greet_visitor(void)")
DOCUMENTED = DOCUMENTED.replace("PROGRAM(100, start);", "PROGRAM(100, greet_visitor);").replace("MAIN(start);", "MAIN(greet_visitor);")


def test_learn_records_names_and_docs():
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "greet.c"
        script.write_text(DOCUMENTED, encoding="utf-8")
        result = learn.learn(script, Path(folder) / "defs", natives.load(), {}, [])
        assert result == {"learned": {"event": 2}, "scripts": 1, "failures": []}
        table = json.loads((Path(folder) / "defs/names/event.json").read_text())["functions"]
        entries = sorted(({key: value for key, value in entry.items() if key != "loose"} for entry in table.values()),
                         key=lambda entry: entry["name"])
        assert all("loose" in entry for entry in table.values())
        assert entries[0] == {"name": "greet_visitor", "locals": ["count", "angle", "slots"],
                              "doc": "Greets the visitor and counts the visit."}
        assert entries[1] == {"name": "helper", "params": ["values", "scale"]}
        data = compile.compile_program(parse.parse(examples.EVENT), natives.load())
        program = decompile.decompile(stb.read(data), "event", natives.load())
        text = source.text(names.apply(program, names.merged(names.load(Path(folder) / "defs/names"))))
        assert "// Greets the visitor and counts the visit.\nvoid greet_visitor(void)" in text
        assert "int helper(int values[], float scale)" in text
        assert compile.compile_program(sdk.resolve(parse.parse(text), natives.load()), natives.load()) == data


def test_learn_skips_default_names():
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "plain.c"
        script.write_text(examples.EVENT, encoding="utf-8")
        assert learn.learn(script, Path(folder) / "defs", natives.load(), {}, [])["learned"] == {"event": 0}


def test_definition_sets_layer():
    with tempfile.TemporaryDirectory() as folder:
        base = Path(folder) / "base"
        mod = Path(folder) / "mod"
        for directory, language, extends in ((base, "en", []), (mod, "de", ["../base"])):
            (directory / "names").mkdir(parents=True)
            (directory / "definitions.json").write_text(json.dumps(
                {"language": language, "extends": extends, "tables": [{"prefix": directory.name + "/", "table": "monster"}]}))
        (base / "names/event.json").write_text(json.dumps({"functions": {"a": {"name": "one"}, "b": {"name": "two"}}}))
        (mod / "names/event.json").write_text(json.dumps({"functions": {"b": {"name": "deux"}}}))
        defs = definitions.load([mod])
        assert [Path(path).name for path in defs["sets"]] == ["base", "mod"]
        assert defs["language"] == "de"
        assert defs["names"]["event"] == {"a": {"name": "one"}, "b": {"name": "deux"}}
        assert defs["tables"] == [["mod/", "monster"], ["base/", "monster"]]


def test_names_reach_variants_that_differ_only_in_strings():
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "greet.c"
        script.write_text(DOCUMENTED, encoding="utf-8")
        learn.learn(script, Path(folder) / "defs", natives.load(), {}, [])
        variant = examples.EVENT.replace('print("hello", v0);', 'print("hallo", v0);')
        data = compile.compile_program(parse.parse(variant), natives.load())
        program = decompile.decompile(stb.read(data), "event", natives.load())
        entries = names.with_loose(names.merged(names.load(Path(folder) / "defs/names")), names.loose_map(program))
        assert "void greet_visitor(void)" in source.text(names.apply(program, entries))
