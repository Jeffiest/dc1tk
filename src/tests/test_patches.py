import json
import tempfile
from pathlib import Path

from c2s import definitions, library, mods, natives, numbers, parse, patches, pathnames, rebuild, vm
from tests.test_index import tree as make_tree
from tests.test_mods import disc_for, recorded

# A map in two languages and a monster, sharing a library, as the decompiler lays them out.
MAP = ('#pragma natives event\n\nint visits;\n\nPROGRAM(100, start);\nPROGRAM(500, talk);\nMAIN(start);\n\n'
       '#include "lib/map/s/s17/greet.c"\n\nvoid start(void)\n{\n    greet();\n}\n\n'
       'void talk(void)\n{\n    int frames;\n\n    frames = 3;\n    while (frames > 0) {\n        wait;\n'
       '        frames = frames - 1;\n    }\n    print("talked");\n}\n')
GREET = 'void greet(void)\n{\n    print("hello");\n}\n'
MONSTER = '#pragma natives monster\n\nPROGRAM(0, start);\nMAIN(start);\n\nvoid start(void)\n{\n    print("roar");\n}\n'
DISC_H = "enum monster_kind {\n    MONSTER_GOLEM = 168,  // model e33a, AI e33a\n};\n"
FILES = {"map/s/s17_valley/s17_1.c": MAP, "map/s/s17_valley/s17_2.c": MAP, "map/s/s17_valley/s17.c": MAP,
         "map/s/s18/s18_1.c": MAP, "lib/map/s/s17/greet.c": GREET, "dungeon/monster/e33a_golem.c": MONSTER,
         "include/disc.h": DISC_H}


def setup(folder, mod_files):
    root = make_tree(folder, FILES)
    pathnames.record(root, {"map/s/s17/s17_1.c": "map/s/s17_valley/s17_1.c", "map/s/s17/s17_2.c": "map/s/s17_valley/s17_2.c",
                            "map/s/s17/s17.c": "map/s/s17_valley/s17.c", "dungeon/monster/e33a.c": "dungeon/monster/e33a_golem.c"})
    dirs = []
    for name, texts in mod_files.items():
        mod = Path(folder) / "mods" / name
        for path, text in texts.items():
            (mod / path).parent.mkdir(parents=True, exist_ok=True)
            (mod / path).write_text(text, encoding="utf-8")
        dirs.append(mod)
    return root, dirs


def program_of(root, plan, script):
    path = root / script
    program = parse.parse(path.read_text(encoding="utf-8"))
    program = library.expand(program, library.read_includes(program, str(path)), {})
    return patches.applied(program, plan["scripts"].get(script, []), script, root, {})


def run(program, name):
    printed = []
    vm.run_function(vm.table_of(program), name, [], [("int", 0)] * program["global_num"], printed)
    return printed


def fails(action, words):
    try:
        action()
    except (ValueError, SystemExit) as error:
        assert words in str(error), str(error)
    else:
        raise AssertionError("did not fail: " + words)


def test_an_override_replaces_a_library_function_in_the_patched_scripts_only():
    patch = ('#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print("hi there");\n}\n')
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"hello": {"s17.c": patch}})
        plan = mods.patch_plan(dirs, root, {})
        # the two language variants, not the Japanese script the PAL game never loads
        assert sorted(plan["scripts"]) == ["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"]
        assert run(program_of(root, plan, "map/s/s17_valley/s17_2.c"), "start") == ["hi there"]
        assert run(program_of(root, plan, "map/s/s18/s18_1.c"), "start") == ["hello"]
        assert (root / "lib/map/s/s17/greet.c").read_text(encoding="utf-8") == GREET


def test_original_runs_the_replaced_function_and_mods_chain():
    first = '#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print("first");\n    original();\n}\n'
    second = '#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    original();\n    print("second");\n}\n'
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"a": {"p.c": first}, "b": {"p.c": second}})
        plan = mods.patch_plan(dirs, root, {})
        assert run(program_of(root, plan, "map/s/s17_valley/s17_1.c"), "start") == ["first", "hello", "second"]


def test_two_mods_replacing_one_function_without_original_are_refused():
    replace = '#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n    print("%s");\n}\n'
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"a": {"p.c": replace % "a"}, "b": {"p.c": replace % "b"}})
        plan = mods.patch_plan(dirs, root, {})
        fails(lambda: program_of(root, plan, "map/s/s17_valley/s17_1.c"), "which mod a overrides too")


def test_overrides_are_checked_against_the_script():
    with tempfile.TemporaryDirectory() as folder:
        for text, words in (('#pragma override\nvoid greeet(void)\n{\n}\n', "close: greet"),
                            ('#pragma override\nvoid greet(int loud)\n{\n}\n', "takes 1 parameters"),
                            ('void greet(void)\n{\n}\n', "put #pragma override before it"),
                            ('void mine(void)\n{\n    original();\n}\n', "only a #pragma override function"),
                            ('PROGRAM(500, mine);\nvoid mine(void)\n{\n}\n', "override talk instead"),
                            ('int visits;\n', "already has a global visits")):
            root, dirs = setup(Path(folder) / str(len(words)), {"m": {"p.c": "#pragma target map s17\n" + text}})
            plan = mods.patch_plan(dirs, root, {})
            fails(lambda: program_of(root, plan, "map/s/s17_valley/s17_1.c"), words)


def test_mods_adding_the_same_names_each_keep_their_own():
    patch = ('#pragma target map s17\n\nint count;\n\nPROGRAM(%d, bump);\n\nvoid bump(void)\n{\n    count = count + %d;\n'
             '    print(count);\n}\n')
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"a": {"p.c": patch % (9000, 1)}, "b": {"p.c": patch % (9001, 10)}})
        program = program_of(root, mods.patch_plan(dirs, root, {}), "map/s/s17_valley/s17_1.c")
        by_number = {entry["no"]: entry["function"] for entry in program["programs"]}
        assert by_number[9000] == "bump" and by_number[9001] == "bump_b"
        assert [declared["name"] for declared in program["globals"]] == ["visits", "count", "count_b"]
        table = vm.table_of(program)
        values = [("int", 0)] * program["global_num"]
        printed = []
        for name in ("bump", "bump_b", "bump"):
            vm.run_function(table, name, [], values, printed)
        assert printed == ["1", "10", "2"]


def test_every_frame_functions_run_before_each_wait():
    patch = ('#pragma target map s17\n\n#pragma every_frame\nvoid tick(void)\n{\n    print("tick");\n}\n')
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"m": {"p.c": patch}})
        program = program_of(root, mods.patch_plan(dirs, root, {}), "map/s/s17_valley/s17_1.c")
        assert run(program, "talk") == ["tick", "tick", "tick", "talked"]
        waiting = patch.replace('print("tick");', "wait;")
        root, dirs = setup(Path(folder) / "waits", {"m": {"p.c": waiting}})
        fails(lambda: program_of(root, mods.patch_plan(dirs, root, {}), "map/s/s17_valley/s17_1.c"), "tick waits")


def test_language_picks_each_variants_text():
    patch = ('#pragma target map s17\n\n#pragma override\nvoid greet(void)\n{\n#if LANGUAGE == LANGUAGE_FRENCH\n'
             '    print("bonjour");\n#else\n    print("hello");\n#endif\n    print(LANGUAGE);\n}\n')
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"m": {"p.c": patch}})
        plan = mods.patch_plan(dirs, root, {})
        assert run(program_of(root, plan, "map/s/s17_valley/s17_1.c"), "start") == ["hello", "1"]
        assert run(program_of(root, plan, "map/s/s17_valley/s17_2.c"), "start") == ["bonjour", "2"]


def test_copies_from_several_languages_keep_each_ones_lines():
    english = 'void f(void)\n{\n    wait;\n    say("Hello");\n}\n'
    french = english.replace("Hello", "Bonjour")
    assert patches.language_text({1: english, 3: english}) == english
    assert patches.language_text({2: french, 1: english}) == (
        'void f(void)\n{\n    wait;\n#if LANGUAGE == LANGUAGE_FRENCH\n    say("Bonjour");\n#else\n    say("Hello");\n'
        '#endif\n}\n')
    longer = french.replace("    wait;\n", "    wait;\n    wait;\n")
    assert patches.language_text({1: english, 2: longer}) == (
        "#if LANGUAGE == LANGUAGE_FRENCH\n" + longer + "#else\n" + english + "#endif\n")
    # the merged text reads back as each language's own
    def body(text, defines=None):
        return [{key: value for key, value in node.items() if key != "line"}
                for node in parse.parse(text, defines)["functions"][0]["body"]]

    for language, text in ((1, english), (2, french)):
        merged = patches.language_text({1: english, 2: french})
        defines = dict({name: str(value) for name, value in patches.LANGUAGES.items()}, LANGUAGE=str(language))
        assert body(merged, defines) == body(text)


def test_targets_pick_monsters_libraries_and_paths():
    with tempfile.TemporaryDirectory() as folder:
        root, _ = setup(folder, {})
        scripts = patches.Scripts(root)
        assert scripts.picked({"kind": "monster", "values": ["golem"]}) == {"dungeon/monster/e33a_golem.c"}
        assert scripts.picked({"kind": "monster", "values": ["MONSTER_GOLEM"]}) == {"dungeon/monster/e33a_golem.c"}
        assert scripts.picked({"kind": "library", "values": ["lib/map/s/s17/greet.c"]}) == {
            "map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c", "map/s/s17_valley/s17.c", "map/s/s18/s18_1.c"}
        assert scripts.picked({"kind": "script", "values": ["map/s/s17/s17_?.c"]}) == {
            "map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"}
        _, dirs = setup(Path(folder) / "none", {"m": {"p.c": "#pragma target map s99\n"}})
        fails(lambda: mods.patch_plan(dirs, root, {}), "picks no script")


def test_numbers_are_given_once_kept_and_never_shared():
    shared = "NEW_FLAG(FIGHT);\nNEW_PROGRAM(SHOW);\n"
    patch = ('#pragma target map s17\n#include "names.h"\n\nPROGRAM(SHOW, show);\n\nvoid show(void)\n{\n'
             '    print(FIGHT, SHOW);\n}\n')
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"a": {"names.h": shared, "p.c": patch},
                                    "b": {"names.h": shared, "p.c": patch, "old.c": "void f(void)\n{\n    set_flag(1025, 1);\n}\n"}})
        lock = Path(folder) / "mods" / numbers.LOCK
        plan = mods.patch_plan(dirs, root, {})
        # b writes flag 1025 by hand, so neither mod is given it
        assert plan["numbers"] == {"a": {"FIGHT": 1024, "SHOW": 9000}, "b": {"FIGHT": 1026, "SHOW": 9001}}
        program = program_of(root, plan, "map/s/s17_valley/s17_1.c")
        assert {entry["no"] for entry in program["programs"]} == {100, 500, 9000, 9001}
        assert run(program, "show") == ["1024 9000"] and run(program, "show_b") == ["1026 9001"]
        assert json.loads(lock.read_text(encoding="utf-8"))["flag"] == {"a/FIGHT": 1024, "b/FIGHT": 1026}
        # b alone keeps its numbers, and a's stay reserved
        assert mods.patch_plan(dirs[1:], root, {})["numbers"] == {"b": {"FIGHT": 1026, "SHOW": 9001}}
        (dirs[0].parent / "c").mkdir()
        (dirs[0].parent / "c" / "p.c").write_text("NEW_FLAG(MINE);\n", encoding="utf-8")
        assert mods.patch_plan([dirs[0].parent / "c"], root, {})["numbers"] == {"c": {"MINE": 1027}}


def test_a_number_the_tree_takes_later_is_reported():
    with tempfile.TemporaryDirectory() as folder:
        root, dirs = setup(folder, {"a": {"p.c": "NEW_FLAG(FIGHT);\n"}})
        assert mods.patch_plan(dirs, root, {})["numbers"] == {"a": {"FIGHT": 1024}}
        (root / "map/s/s18/s18_1.c").write_text(MAP.replace('print("talked");', "set_flag(1024, 1);"), encoding="utf-8")
        fails(lambda: mods.patch_plan(dirs, root, {}), "which something else now uses")


def test_build_compiles_patched_scripts_and_override_starts_a_patch():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        iso_path = disc_for(folder, root)
        mod = Path(folder) / "mods" / "quiet"
        result = mods.override(root, mod, "script", "map/m/t01.c", ["helper"])
        assert result["added"] == ["helper"] and result["scripts"] == ["map/m/t01.c"]
        text = result["path"].read_text(encoding="utf-8")
        assert "#pragma target script map/m/t01.c" in text and "#pragma override\nvoid helper(void)\n{\n    wait;\n}" in text
        assert mods.override(root, mod, "script", "map/m/t01.c", ["helper"])["added"] == []
        result["path"].write_text(text.replace("    wait;\n", "    wait;\n    wait;\n"), encoding="utf-8")
        plan = mods.patch_plan([mod], root, {})
        found = rebuild.changes(root, iso_path, definitions.empty(), {}, natives.load(), None, sdk_dir=None,
                                patched=plan["scripts"])
        assert [change["source"] for change in found["scripts"]] == ["map/m/t01.c"]
        assert mods.check([mod], root, definitions.empty(), natives.load())["errors"] == []
        assert "patch t01.c: 1 script; overrides helper" in mods.listing(Path(folder) / "mods", root)
