import tempfile
from pathlib import Path

from c2s import definitions, natives

HEADER = """
enum TestCommand {
    TEST_EXT_END = -1, /**< Ends the table. */
    TEST_EXT_GET_FLAG = 0x0C, /**< Runs _GET_FLAG. */
    TEST_EXT_SET_FLAG = 3,
};
"""

SOURCE = """
int _SET_FLAG(RS_STACKDATA *stack, int arg_count) {
    int bit = GetStackInt(stack++);
    float value = GetStackFloat(stack);
    if (bit) {
        value = 0;
    }
    return 1;
}

int _GET_FLAG(RS_STACKDATA *stack, int arg_count) {
    char *name = GetStackString(stack++);
    SetStack(stack, 1);
    return 1;
}

static RS_EXTFUNC_INFO ext_func_info[3] = {
    {_GET_FLAG, TEST_EXT_GET_FLAG},
    {_SET_FLAG, TEST_EXT_SET_FLAG},
    {NULL,      TEST_EXT_END     },
};
"""

SPEC = {"table": "test", "source": "src/test.cpp", "header": "include/test.hpp", "array": "ext_func_info"}


def test_builds_table_from_enum_and_rows():
    with tempfile.TemporaryDirectory() as folder:
        root = Path(folder)
        (root / "src").mkdir()
        (root / "include").mkdir()
        (root / "src/test.cpp").write_text(SOURCE)
        (root / "include/test.hpp").write_text(HEADER)
        table = natives.build_table(root, SPEC)
    assert table["natives"] == [
        {"no": 3, "name": "_SET_FLAG", "accessors": ["int", "float"]},
        {"no": 12, "name": "_GET_FLAG", "accessors": ["string", "out"]},
    ]


def test_function_body_matches_braces():
    body = natives.function_body(SOURCE, "_SET_FLAG")
    assert "value = 0;" in body
    assert "_GET_FLAG" not in body
    assert natives.function_body(SOURCE, "_MISSING") is None


def test_table_for_path():
    rules = definitions.load([definitions.DEFAULT_SET])["tables"]
    assert natives.table_for(rules=rules, path="map/s/s40/s40.stb") == "event"
    assert natives.table_for(rules=rules, path="dungeon/sys_script/d05.stb") == "event"
    assert natives.table_for(rules=rules, path="dungeon/monster/b02a.stb") == "monster"
    assert natives.table_for(rules=rules, path="dungeon/articles/a01.stb") == "effect"
    assert natives.table_for(rules=rules, path="mainchr/c01.stb") == "action"
    assert natives.table_for(rules=rules, path="event/test_arg.stb") == "arg"
    assert natives.table_for(rules=rules, path="event/tbox/bomb.pack.d/tbox_exp.stb") == "effect"


def test_generated_tables_are_present():
    tables = natives.load()
    counts = {name: len(table["natives"]) for name, table in tables.items()}
    assert counts == {"event": 696, "monster": 173, "action": 82, "effect": 128, "arg": 2}
    for table in tables.values():
        numbers = [native["no"] for native in table["natives"]]
        names = [native["name"] for native in table["natives"]]
        assert len(set(numbers)) == len(numbers)
        assert len(set(names)) == len(names)
    assert natives.numbered(tables["event"])[12]["name"] == "_ASSIGN_STACK"
