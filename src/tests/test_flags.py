import tempfile

from c2s import flags
from tests.test_index import tree

FILES = {
    "map/m.c": "void start(void)\n{\n    set_flag(FLAG_DOOR_OPEN, 1);\n    open = get_flag(FLAG_DOOR_OPEN);\n"
               "    set_flag(base + i, 1);\n}\n",
    "dungeon/monster/e1.c": "void tick(void)\n{\n    on = get_dungeon_flag(2040);\n}\n",
}
CONSTS = {"FLAG_DOOR_OPEN": 900}


def test_scan_counts_literal_and_named_flags_and_lists_computed_ones():
    with tempfile.TemporaryDirectory() as folder:
        found = flags.scan(tree(folder, FILES), CONSTS)
        assert sorted(found["uses"]) == [900, 2040]
        assert [use["access"] for use in found["uses"][900]] == ["set", "get"]
        assert [use["expr"] for use in found["computed"]] == ["base + i"]


def test_free_ranges_leave_out_scripts_engine_and_tools():
    with tempfile.TemporaryDirectory() as folder:
        found = flags.scan(tree(folder, FILES), CONSTS)
        used = flags.used_numbers(found)
        assert {900, 2040, 799, 2047} <= used
        ranges = flags.free_ranges(used)
        assert (2041, 2046) in ranges
        assert all(not (a <= 900 <= b) for a, b in ranges)
        assert "1 calls pass a flag number worked out at run time" in flags.summary(found)


def test_who_names_the_flag_and_every_use():
    with tempfile.TemporaryDirectory() as folder:
        found = flags.scan(tree(folder, FILES), CONSTS)
        text = flags.who(found, 900, CONSTS)
        assert text.startswith("flag 900 (FLAG_DOOR_OPEN)")
        assert "set  map/m.c:3  set_flag(FLAG_DOOR_OPEN, 1)  (in start)" in text
        assert "free" in flags.who(found, 1500, CONSTS)
        assert "engine: charasetup.cpp" in flags.who(found, 799, CONSTS)
