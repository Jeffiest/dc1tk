import tempfile
from pathlib import Path

from c2s import index


def tree(folder, files):
    root = Path(folder) / "tree"
    for path, text in files.items():
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(text, encoding="utf-8")
    return root


def test_constants_come_from_enums_and_defines():
    texts = {"a.h": "enum program {\n    map_start = 100,\n    story_50 = 10500,  // note\n};\n#define LIMIT 0x10\n",
             "b.h": "static inline void f(int x)\n{\n    x = 5;\n}\n"}
    assert index.constants(texts) == {"map_start": 100, "story_50": 10500, "LIMIT": 16}


def test_values_fold_like_the_compiler():
    consts = {"story_50": 10500, "BASE": 8}
    assert index.value("50 * 10 + 0 + 10000", consts) == 10500
    assert index.value("story_50", consts) == 10500
    assert index.value("-(BASE + 1)", consts) == -9
    assert index.value("7 / 2", consts) == 3
    assert index.value('"s51/s51.stb"', consts) == "s51/s51.stb"
    assert index.value("base + i", consts) is None
    assert index.value("get(1)", consts) is None


def test_calls_carry_arguments_lines_and_functions_and_skip_comments():
    with tempfile.TemporaryDirectory() as folder:
        root = tree(folder, {"map/m.c": '// calls set_flag(1, 1) in a comment\nvoid start(void)\n{\n'
                                        '    set_flag(FLAG_A + 1, get(2, 3));\n    print("set_flag(9)");\n}\n'})
        found = index.calls(root, ["set_flag"])
        assert len(found) == 1
        assert found[0]["path"] == "map/m.c"
        assert found[0]["line"] == 4
        assert found[0]["function"] == "start"
        assert found[0]["args"] == ["FLAG_A + 1", "get(2, 3)"]


def test_includers_follow_libraries_through_libraries():
    with tempfile.TemporaryDirectory() as folder:
        root = tree(folder, {"map/a_1.c": 'PROGRAM(500, go);\n#include "lib/outer.c"\n',
                             "map/b.c": '#include "lib/inner.c"\n',
                             "lib/outer.c": '#include "lib/inner.c"\n', "lib/inner.c": "void f(void)\n{\n}\n"})
        outline = index.outlines(root)
        assert outline["map/a_1.c"]["programs"] == [{"number": "500", "function": "go"}]
        assert index.includers(outline) == {"lib/outer.c": ["map/a_1.c"], "lib/inner.c": ["map/a_1.c", "map/b.c"]}


def test_language_variants_share_a_key():
    assert index.variant_key("map/s/s51/s51_3.c") == index.variant_key("map/s/s51/s51_1.c") == "map/s/s51/s51_N.c"
    assert index.variant_key("map/s/s51/s51.c") == "map/s/s51/s51.c"
