import json
import tempfile
from pathlib import Path

from c2s import compile, decompile, definitions, learn, library, names, natives, parse, sdk, source, stb
from tests import test_source as examples

DOCUMENTED = examples.NAMED.replace("void start(void)", "// Greets the visitor and counts the visit.\nvoid greet_visitor(void)")
DOCUMENTED = DOCUMENTED.replace("PROGRAM(100, start);", "PROGRAM(100, greet_visitor);").replace("MAIN(start);", "MAIN(greet_visitor);")


def test_learn_records_names_and_docs():
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "greet.c"
        script.write_text(DOCUMENTED, encoding="utf-8")
        result = learn.learn(script, Path(folder) / "defs", natives.load(), {}, [])
        assert result == {"learned": {"event": 2}, "scripts": 1, "libraries": 0, "failures": []}
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


def test_copies_of_a_function_resolve_to_its_edit_not_its_alias():
    existing = {"name": "start_event", "doc": "Old doc."}
    copies = [{"name": "start_event", "doc": "Old doc."}, {"name": "start_event_for_program_506", "doc": "New doc."},
              {"name": "start_event_2"}]
    assert learn.resolved(copies, existing) == {"name": "start_event", "doc": "New doc."}
    assert learn.resolved([{"name": "start_event"}, {"name": "begin_event"}], existing) == {"name": "begin_event"}


def test_docs_wrapped_at_a_hyphen_join_without_a_space():
    comments = [(1, "Pushes backwards while the flip is mid-"), (2, "motion, then stops - briefly.")]
    assert parse.comment_block(comments, 3) == "Pushes backwards while the flip is mid-motion, then stops - briefly."


def test_a_handler_takes_the_name_of_the_program_it_handles():
    program = {"functions": [{"name": "f_0000", "kind": "function"}, {"name": "f_0010", "kind": "function"}],
               "programs": [{"no": 102, "function": "f_0000"}], "main": "f_0010"}
    mapping = names.assign(program, {"f_0000": "abc", "f_0010": None}, {"abc": "on_dodge_shot"}, set(),
                           {101: "dodge_shot", 102: "dodge_melee"})
    assert mapping["f_0000"] == "on_dodge_melee"


def test_a_set_knows_which_disc_it_was_made_for():
    with tempfile.TemporaryDirectory() as folder:
        disc, set_dir = Path(folder) / "disc", Path(folder) / "set"
        (disc / "map").mkdir(parents=True)
        set_dir.mkdir()
        (disc / "map" / "a.stb").write_bytes(b"one")
        (disc / "map" / "b.stb").write_bytes(b"two")
        assert definitions.write_disc(disc, set_dir) == 2
        defs = definitions.load([set_dir])
        assert definitions.disc_differences(disc, defs) == {"differ": [], "missing": [], "unknown": []}
        (disc / "map" / "a.stb").write_bytes(b"changed")
        (disc / "map" / "b.stb").unlink()
        (disc / "map" / "c.stb").write_bytes(b"new")
        assert definitions.disc_differences(disc, defs) == {"differ": ["map/a.stb"], "missing": ["map/b.stb"],
                                                            "unknown": ["map/c.stb"]}


def test_libraries_are_keyed_by_the_functions_they_hold():
    var = lambda name: {"kind": "expr", "expr": {"kind": "var", "name": name}}
    function = lambda name, body, path=None, locals_=(): {"name": name, "library": path, "params": [], "body": body,
                                                         "locals": [{"name": local} for local in locals_]}
    functions = [function("a", [var("visits")]), function("b", [var("visits")], "lib/x.c"),
                 function("c", [var("g1"), var("talks")], "lib/x.c", ["talks"]), function("d", [], "lib/y.c")]
    found = {0: "f0", 16: "f1", 32: "f2", 48: "f3"}
    # the script's globals, visits and talks, decompile as g0 and g1; talks is a local in c
    entries = learn.library_entries(functions, found, "event", {"lib/x.c": "Shared helpers."},
                                    {"visits": "g0", "talks": "g1"})
    assert entries == {library.library_fingerprint("event", ["f1", "f2"]): {"path": "lib/x.c", "doc": "Shared helpers.",
                                                                            "globals": {"g0": "visits"}},
                       library.library_fingerprint("event", ["f3"]): {"path": "lib/y.c"}}


TYPED_HEADER = """#include <c2s.h>
#pragma natives event
typedef int music;
enum music_pal {
    MUSIC_TOWN = 22,
};
extern void load_bgm(int file) NATIVE(610);
"""

TYPED = """#include <event.h>

PROGRAM(100, on_init);
MAIN(on_init);

void on_init(void)
{
    music track;

    track = 22;
    if (track == 22) {
        load_bgm(track);
    }
}
"""


def test_types_declared_in_source_come_back_with_their_names():
    headers = {"event.h": TYPED_HEADER, "c2s.h": "#define NATIVE(...)\n"}
    tables = natives.load()
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "typed.c"
        script.write_text(TYPED, encoding="utf-8")
        learn.learn(script, Path(folder) / "defs", tables, headers, [])
        entries = names.merged(names.load(Path(folder) / "defs/names"))
        assert [entry.get("types") for entry in entries.values()] == [{"track": "music"}]
        data = compile.compile_program(library.expand(parse.parse(TYPED), {}, headers), tables)
        assert data == compile.compile_program(library.expand(parse.parse(TYPED.replace("music track", "int track")), {},
                                                              headers), tables)
        found = sdk.header_context("event", headers, tables)
        program = decompile.decompile(stb.read(data), "event", tables)
        text = source.text(sdk.present(names.apply(program, entries, sdk.reserved(found), found["program_names"]), found))
        assert "    music track;\n" in text and "track = MUSIC_TOWN;" in text and "track == MUSIC_TOWN" in text


def test_a_declared_return_type_is_learned_when_it_differs():
    headers = {"event.h": TYPED_HEADER, "c2s.h": "#define NATIVE(...)\n"}
    text = TYPED.replace("void on_init(void)", "float scale(void)\n{\n    return 2;\n}\n\nvoid on_init(void)")
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "typed.c"
        script.write_text(text, encoding="utf-8")
        learn.learn(script, Path(folder) / "defs", natives.load(), headers, [])
        entries = names.merged(names.load(Path(folder) / "defs/names")).values()
        assert [entry.get("returns") for entry in entries if entry.get("name") == "scale"] == ["float"]
        assert all("returns" not in entry for entry in entries if entry.get("name") == "on_init")


def test_line_notes_come_back_above_their_statements():
    headers = {"event.h": TYPED_HEADER, "c2s.h": "#define NATIVE(...)\n"}
    text = TYPED.replace("    if (track == 22) {", "    // note: 22 is the town theme.\n    if (track == 22) {")
    tables = natives.load()
    with tempfile.TemporaryDirectory() as folder:
        script = Path(folder) / "noted.c"
        script.write_text(text, encoding="utf-8")
        learn.learn(script, Path(folder) / "defs", tables, headers, [])
        entries = names.merged(names.load(Path(folder) / "defs/names"))
        assert [entry.get("notes") for entry in entries.values()] == [{"4": "22 is the town theme."}]
        data = compile.compile_program(library.expand(parse.parse(text), {}, headers), tables)
        found = sdk.header_context("event", headers, tables)
        program = decompile.decompile(stb.read(data), "event", tables)
        shown = source.text(sdk.present(names.apply(program, entries, sdk.reserved(found), found["program_names"]), found))
        assert "    // note: 22 is the town theme.\n    if (track == MUSIC_TOWN) {" in shown
        assert compile.compile_program(library.expand(parse.parse(shown), {}, headers), tables) == data
