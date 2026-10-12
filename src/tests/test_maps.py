import tempfile
from pathlib import Path

from c2s import definitions, maps
from tests.test_index import tree as make_tree

MAP_FILE = """PARTS_NAME "p01_s99";
FUNC_POINT 2;
FUNC_DATA "event",1;
FUNC_NAME "talk";
FUNC_FLAG 0, 0, 0,0;
FUNC_EVENT_DATA "event",0,500,0,0,0,"",1,0,0;
FUNC_POS 68.04,41.85,1180.61, 0,0,0, 30,30,30;
FUNC_DATA_END;
FUNC_DATA "event",0;
FUNC_NAME "step";
FUNC_EVENT_DATA "event",1,520,0,0,0,"",0,0,0;
FUNC_POS -259.7,0,792.4, 0,0,0, 20,20,20;
FUNC_DATA_END;
FUNC_POINT_END;
"""


def test_trigger_points_give_their_program_and_how_they_start():
    with tempfile.TemporaryDirectory() as folder:
        path = Path(folder) / "s99.map"
        path.write_bytes(MAP_FILE.encode("ascii"))
        points = maps.trigger_points(path)
        assert [(point["name"], point["part"], point["program"], point["check"]) for point in points] == [
            ("talk", "p01_s99", 500, "on the action button"), ("step", "p01_s99", 520, "on touch")]
        assert points[0]["position"] == [68.04, 41.85, 1180.61] and points[1]["size"] == [20.0, 20.0, 20.0]


def test_the_catalog_says_how_the_game_uses_a_map_and_what_is_free():
    with tempfile.TemporaryDirectory() as folder:
        root = make_tree(Path(folder) / "tree", {
            "map/s/s98/s98_1.c": "PROGRAM(map_start, start);\nPROGRAM(player_dead, lose);\nPROGRAM(9000, mine);\n",
            "map/s/s97/s97_1.c": "PROGRAM(map_start, start);\n",
            "map/d/d01/d01_1.c": 'void warp(void)\n{\n    load_dungeon_map("s98");\n    load_event_script("s98/s98.stb", 9000);\n'
                                 '    jump_to_map(0, "s97", map_start);\n}\n'})
        consts = {"map_start": 100, "player_dead": 1200}
        found = maps.catalog(root, Path(folder) / "no-disc", definitions.empty(), consts)
        assert found["s98"]["kinds"] == ["dungeon-style"] and found["s98"]["playable"]
        assert found["s97"]["kinds"] == ["town"] and not found["s97"]["playable"]
        text = maps.detail("s98", found, root, Path(folder) / "no-disc", dict(consts, ITEM_SOMETHING=9001), {9002})
        assert "free for a mod: 9001, 9003" in text and "starts program 9000" in text


def test_doors_and_boxes_name_the_programs_they_run():
    door = {"program": None, "check": None, "name": "", "event": ['"door"', "0", "200", "0", "3", "210", '"i01h01"', "1", "0", "1"]}
    box = {"program": None, "check": None, "name": "", "event": ['"t_box"', "0", "0", "388", "1", "105", '""', "1", "0", "1"]}
    assert maps.point_programs(door) == [(200, "door to i01h01"), (210, "door to i01h01")]
    assert maps.point_programs(box) == [(105, "treasure box")]


def test_program_lines_carry_their_trigger_as_a_comment():
    from c2s import parse, source
    program = parse.parse("#pragma natives event\n\nPROGRAM(520, leave);\nMAIN(leave);\n\nvoid leave(void)\n{\n}\n")
    program["programs"][0]["note"] = 'trigger point "exit" (action button)'
    assert 'PROGRAM(520, leave);  // trigger point "exit" (action button)' in source.text(program)
