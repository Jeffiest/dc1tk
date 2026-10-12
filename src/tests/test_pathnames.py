import tempfile
from pathlib import Path

from c2s import definitions, natives, pathnames, rebuild, tree, xref
from tests import test_rebuild as disc_fixtures
from tests.test_index import tree as make_tree

FOUND = {"maps": {"s51": "Raybrandt Castle", "i01h01": "Cedric's Shop", "s56": "Ixion"},
         "ais": {"e33a": "Golem", "b254a": "Griffon Soldier"},
         "titles": {"map/d/d01": "Underground Channel", "mainchr/c01.stb": "Max", "map/i/i01": "Shops"}}


def test_names_are_plain_words_after_the_code():
    assert pathnames.slug("Ocean's Roar Cave") == "oceans_roar_cave"
    assert pathnames.slug("Dr.Dell") == "dr_dell"
    assert pathnames.named_script("map/s/s51/s51_1.c", FOUND) == "map/s/s51_raybrandt_castle/s51_1.c"
    assert pathnames.named_script("map/i/i01/h01/i01h01_1.c", FOUND) == "map/i/i01_shops/h01_cedrics_shop/i01h01_1.c"
    assert pathnames.named_script("map/d/d01/d01_1.c", FOUND) == "map/d/d01_underground_channel/d01_1.c"
    assert pathnames.named_script("map/s/s56.c", FOUND) == "map/s/s56_ixion.c"
    assert pathnames.named_script("dungeon/monster/e33a.c", FOUND) == "dungeon/monster/e33a_golem.c"
    assert pathnames.named_script("mainchr/c01.c", FOUND) == "mainchr/c01_max.c"
    assert pathnames.named_script("dungeon/eff_script/ac_spark.c", FOUND) == "dungeon/eff_script/ac_spark.c"


def test_libraries_name_their_folders_like_the_scripts_they_serve():
    assert pathnames.named_library("lib/map/s/s51/monica_gear_battle.c", FOUND) == \
        "lib/map/s/s51_raybrandt_castle/monica_gear_battle.c"
    assert pathnames.named_library("lib/dungeon/monster/death_motion.c", FOUND) == "lib/dungeon/monster/death_motion.c"


def test_monster_ai_scripts_take_the_name_of_the_kind_whose_model_they_are():
    rows = {168: {"name": "Golem", "model": "e33a", "script": "e33a"},
            169: {"name": "Rave Golem", "model": "e33b", "script": "e33a"},
            5: {"name": "Mini Golem", "model": "e40a", "script": "e33a"},
            523: {"name": "Griffon Soldier", "model": "e254a", "script": "b254a"}}
    assert pathnames.ai_names(rows) == {"e33a": "Golem", "b254a": "Griffon Soldier"}


def test_a_tree_records_what_each_named_path_was():
    with tempfile.TemporaryDirectory() as folder:
        pathnames.record(folder, {"map/s/s51/s51_1.c": "map/s/s51_raybrandt_castle/s51_1.c", "map/t01.c": "map/t01.c"})
        assert pathnames.unnamed(folder, "map/s/s51_raybrandt_castle/s51_1.c") == "map/s/s51/s51_1.c"
        assert pathnames.unnamed(folder, "map/t01.c") == "map/t01.c"
        assert pathnames.named(folder, "map/s/s51/s51_1.c") == "map/s/s51_raybrandt_castle/s51_1.c"


def test_build_finds_the_disc_script_of_a_named_file():
    with tempfile.TemporaryDirectory() as folder:
        iso_path = disc_fixtures.disc(folder, 4)
        root = disc_fixtures.source(folder, {"map/t01_town.c": "    wait;\n    wait;\n", "map/t02.c": "    end;\n"})
        pathnames.record(root, {"map/t01.c": "map/t01_town.c"})
        found = rebuild.changes(root, iso_path, definitions.empty(), {}, natives.load())
        assert [(change["source"], change["top"]) for change in found["scripts"]] == [("map/t01_town.c", "map/t01.stb")]
        assert found["skipped"] == []


def test_xref_matches_named_folders_by_their_code():
    with tempfile.TemporaryDirectory() as folder:
        root = make_tree(folder, {"map/s/s51_raybrandt_castle/s51_1.c": "PROGRAM(510, end_fight);\n",
                                  "dungeon/monster/b254a_griffon_soldier.c": "void on_death(void)\n{\n    run_event_script(510);\n}\n",
                                  "map/s/s51_raybrandt_castle/s51_2.c": "void f(void)\n{\n    place_monster(523);\n}\n"})
        pathnames.record(root, {"map/s/s51/s51_1.c": "map/s/s51_raybrandt_castle/s51_1.c",
                                "map/s/s51/s51_2.c": "map/s/s51_raybrandt_castle/s51_2.c",
                                "dungeon/monster/b254a.c": "dungeon/monster/b254a_griffon_soldier.c"})
        rows = {523: {"name": "Griffon Soldier", "model": "e254a", "script": "b254a"}}
        text = xref.event_report(root, {}, 510, "s51", rows)
        assert "map/s/s51_raybrandt_castle/s51_1.c  PROGRAM(510, end_fight)" in text
        assert "<- s51 places kind 523 Griffon Soldier" in text
        assert "AI script: dungeon/monster/b254a_griffon_soldier.c" in xref.monster_report(root, {}, rows, "523")
