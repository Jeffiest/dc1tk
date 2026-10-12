import json
import tempfile
from pathlib import Path

from c2s import additions, mods, pathnames
from tests.test_index import tree as make_tree

SCRIPT = ("#pragma natives event\n\nPROGRAM(map_start, start);\nPROGRAM(500, talk);\nMAIN(start);\n\n"
          "void start(void)\n{\n}\n\nvoid talk(void)\n{\n}\n\n#pragma padding_of talk 00000000\n")
CONSTS = {"map_start": 100, "treasure_chest": 1100}


def test_scripts_are_picked_by_map_or_pattern_but_never_libraries():
    assert additions.matches({"map": "s17"}, "map/s/s17/s17_3.c")
    assert additions.matches({"map": "s17"}, "map/s/s17/s17.c")
    assert not additions.matches({"map": "s17"}, "map/s/s18/s18_1.c")
    assert additions.matches({"scripts": ["map/s/s17/s17_?.c"]}, "map/s/s17/s17_1.c")
    assert not additions.matches({"scripts": ["map/s/s17/s17_?.c"]}, "map/s/s17/s17.c")
    assert not additions.matches({"map": "s17"}, "lib/map/s/s17/helpers.c")


def test_programs_and_includes_are_added_before_main_and_the_padding_pragmas():
    text = additions.applied(SCRIPT, {"programs": {"9000": "go", "treasure_chest": "open"},
                                      "includes": ["lib/mods/mine/mine.c"]}, "mine", CONSTS)
    assert "PROGRAM(500, talk);\nPROGRAM(9000, go);\nPROGRAM(treasure_chest, open);\nMAIN(start);" in text
    assert text.index('#include "lib/mods/mine/mine.c"') < text.index("#pragma padding_of talk")


def test_a_program_the_script_has_cannot_be_added_again():
    for program in ("500", "map_start", "100"):
        try:
            additions.applied(SCRIPT, {"programs": {program: "mine"}}, "mine", CONSTS)
        except SystemExit as error:
            assert "wrap it instead" in str(error)
        else:
            raise AssertionError("added program %s twice" % program)


def test_wrapping_runs_functions_around_a_program():
    text = additions.applied(SCRIPT, {"wrap": {"map_start": {"before": ["first"], "after": ["last"]},
                                               "1100": {"before": ["chest"]}}}, "my-mod", CONSTS)
    assert "PROGRAM(map_start, mod_my_mod_map_start);" in text
    assert "void mod_my_mod_map_start(void)\n{\n    first();\n    start();\n    last();\n}" in text
    assert "PROGRAM(1100, mod_my_mod_1100);" in text
    assert "void mod_my_mod_1100(void)\n{\n    chest();\n}" in text


def test_several_mods_can_add_to_one_script_and_a_mod_needs_no_copies():
    with tempfile.TemporaryDirectory() as folder:
        root = make_tree(Path(folder) / "tree", {"map/s/s17_valley/s17_1.c": SCRIPT, "map/s/s17_valley/s17_2.c": SCRIPT})
        pathnames.record(root, {"map/s/s17/s17_1.c": "map/s/s17_valley/s17_1.c", "map/s/s17/s17_2.c": "map/s/s17_valley/s17_2.c"})
        first, second = Path(folder) / "mods" / "first", Path(folder) / "mods" / "second"
        for mod, program in ((first, "9000"), (second, "9001")):
            mod.mkdir(parents=True)
            (mod / mods.INDEX).write_text(json.dumps({"additions": [{"map": "s17", "programs": {program: "go"}}]}),
                                          encoding="utf-8")
        texts, clashes = mods.overlay([first, second], root, CONSTS)
        assert clashes == {} and sorted(texts) == ["map/s/s17_valley/s17_1.c", "map/s/s17_valley/s17_2.c"]
        assert "PROGRAM(9000, go);\nPROGRAM(9001, go);\nMAIN" in texts["map/s/s17_valley/s17_2.c"]
        assert "adds to s17: program 9000" in mods.listing(Path(folder) / "mods", root)


def test_wrapping_a_program_twice_in_one_mod_gives_each_wrapper_its_own_name():
    once = additions.applied(SCRIPT, {"wrap": {"map_start": {"before": ["first"]}}}, "mine", CONSTS)
    twice = additions.applied(once, {"wrap": {"map_start": {"before": ["second"]}}}, "mine", CONSTS)
    assert "PROGRAM(map_start, mod_mine_map_start_2);" in twice
    assert "void mod_mine_map_start_2(void)\n{\n    second();\n    mod_mine_map_start();\n}" in twice
