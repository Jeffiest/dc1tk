import tempfile

from c2s import asm, definitions, natives, rebuild, sdk, startat, stb
from tests.test_index import tree

NEW_GAME = """#include <event.h>

PROGRAM(floor_start, setup_floor);
MAIN(setup_floor);

void setup_floor(void)
{
    wait;
}

#pragma padding_of setup_floor 00000000
"""


def test_targets_name_a_map_an_optional_script_and_a_program():
    assert startat.parse_target("s51:story_50") == {"map": "s51", "script": "s51", "program": "story_50"}
    assert startat.parse_target("s06/s06b:500") == {"map": "s06", "script": "s06b", "program": "500"}
    try:
        startat.parse_target("s51")
        raise AssertionError("a target without a program should be refused")
    except SystemExit:
        pass


def test_patch_warps_once_then_runs_the_original_floor_start():
    text = startat.patched(NEW_GAME, startat.parse_target("s51:story_50"))
    assert "PROGRAM(floor_start, start_at_scene);" in text
    assert text.index("void start_at_scene(void)") < text.index("#pragma padding_of")
    assert '        load_event_script("s51/s51.stb", story_50);' in text
    assert "    setup_floor();" in text


def test_patched_new_game_script_compiles_with_the_warp():
    with tempfile.TemporaryDirectory() as folder:
        root = tree(folder, {"map/d/d01/d01_1.c": NEW_GAME, "map/d/d01/d01_2.c": NEW_GAME, "map/s/s51/s51_1.c": NEW_GAME})
        overrides = startat.overrides(root, startat.parse_target("s51:story_50"))
        assert sorted(overrides) == ["map/d/d01/d01_1.c", "map/d/d01/d01_2.c"]
        data = rebuild.compiled(root / "map/d/d01/d01_1.c", definitions.empty(), sdk.load_texts(), natives.load(),
                                overrides["map/d/d01/d01_1.c"])
        text = asm.disassemble(stb.read(data), "event", natives.load())
        assert "s51/s51.stb" in text and "push 10500" in text and "push %d" % startat.START_FLAG in text


def test_warns_when_the_game_never_loads_the_map_that_way():
    with tempfile.TemporaryDirectory() as folder:
        root = tree(folder, {"map/d/d01/d01_1.c": 'void w(void)\n{\n    load_dungeon_map("s51");\n}\n'})
        assert startat.warnings(root, {"story_50": 10500}, startat.parse_target("s51:story_50")) == []
        found = startat.warnings(root, {}, startat.parse_target("t01:map_start"))
        assert len(found) == 2 and "never loads t01" in found[1]
