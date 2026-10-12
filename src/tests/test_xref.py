import tempfile

from c2s import xref
from tests.test_index import tree

FILES = {
    "map/s/s99/s99_1.c": 'PROGRAM(510, end_fight);\nPROGRAM(story_9, start_fight);\n#include "lib/map/s/s99/fight.c"\n',
    "map/s/s99/s99_2.c": 'PROGRAM(510, end_fight);\nPROGRAM(story_9, start_fight);\n#include "lib/map/s/s99/fight.c"\n',
    "lib/map/s/s99/fight.c": "void start_fight(void)\n{\n    load_monster_data(MONSTER_GUARD, 1);\n"
                             "    place_monster(MONSTER_GUARD, 1.0, 0.0, 2.0);\n}\n",
    "map/d/d01/d01_1.c": 'void warp(void)\n{\n    load_dungeon_map("s99");\n    load_event_script("s99/s99.stb", 9 * 10 + 10000);\n}\n',
    "dungeon/monster/g7.c": "void on_death(void)\n{\n    run_event_script(510);\n}\n",
    "dungeon/monster/x1.c": "void on_death(void)\n{\n    run_event_script(510);\n}\n",
}
CONSTS = {"story_9": 10090, "MONSTER_GUARD": 7}
ROWS = {7: {"name": "Guard", "model": "e7", "script": "g7"}, 8: {"name": "Elite Guard", "model": "e8", "script": "g7"}}


def test_event_report_links_a_map_program_to_the_ai_that_requests_it():
    with tempfile.TemporaryDirectory() as folder:
        text = xref.event_report(tree(folder, FILES), CONSTS, 510, "s99", ROWS)
        assert "map/s/s99/s99_N.c  PROGRAM(510, end_fight)" in text
        assert "dungeon/monster/g7.c:3  run_event_script(510)  (in on_death)\n      <- s99 places kind 7 Guard" in text
        assert "dungeon/monster/x1.c:3  run_event_script(510)  (in on_death)\n" in text + "\n"
        assert "x1.c:3  run_event_script(510)  (in on_death)\n      <-" not in text


def test_event_report_finds_warps_that_start_the_program():
    with tempfile.TemporaryDirectory() as folder:
        text = xref.event_report(tree(folder, FILES), CONSTS, 10090, "s99", ROWS)
        assert "PROGRAM(10090, start_fight)" in text
        assert 'map/d/d01/d01_1.c:4  load_event_script("s99/s99.stb", 9 * 10 + 10000)  (in warp)' in text


def test_map_report_lists_scripts_and_entries():
    with tempfile.TemporaryDirectory() as folder:
        text = xref.map_report(tree(folder, FILES), CONSTS, "s99")
        assert "  map/s/s99/s99_1.c\n  map/s/s99/s99_2.c" in text
        assert 'load_dungeon_map("s99")' in text
        assert "starts program 10090" in text


def test_monster_report_gives_files_sharing_and_users():
    with tempfile.TemporaryDirectory() as folder:
        text = xref.monster_report(tree(folder, FILES), CONSTS, ROWS, "guard")
        assert text.startswith("monster kind 7 Guard (MONSTER_GUARD)")
        assert "AI script: dungeon/monster/g7.c" in text
        assert "the same AI script also runs kinds: 8 Elite Guard" in text
        assert "lib/map/s/s99/fight.c:3  load_monster_data(MONSTER_GUARD, 1)" in text
        assert "library included by 2 scripts: map/s/s99/s99_N.c" in text
