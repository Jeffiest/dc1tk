import tempfile
from pathlib import Path

from c2s import definitions, mods, natives, pathnames, rebuild, sdk, source, tree
from tests import build
from tests.test_index import tree as make_tree

SCRIPT = '#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\n#include "lib/m/shared.c"\n\nvoid f(void)\n{\n    helper();\n}\n'
LIBRARY = "void helper(void)\n{\n    wait;\n}\n"
FILES = {"map/m/t01.c": SCRIPT, "map/m/t02.c": SCRIPT.replace("PROGRAM(100", "PROGRAM(200"), "lib/m/shared.c": LIBRARY}


def recorded(folder, files=FILES):
    root = make_tree(Path(folder) / "tree", files)
    tree.record(root, list(files))
    return root


def disc_for(folder, root):
    # A disc whose scripts are what the tree compiles to, so an unchanged tree changes nothing.
    data = {name: rebuild.compiled(root / ("map/m/%s.c" % name), definitions.empty(), {}, natives.load()) for name in ("t01", "t02")}
    index, dat = build.archive_files([("\\map\\m\\t01.stb", data["t01"]), ("\\map\\m\\t02.stb", data["t02"])])
    path = Path(folder) / "disc.iso"
    path.write_bytes(build.iso_image([("DATA.HD4", index), ("DATA.DAT", dat), ("DMMY", bytes(4 * build.SECTOR))]))
    return path


def test_new_copies_files_to_edit_and_keeps_the_mods_own_copies():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        mod = Path(folder) / "mods" / "faster"
        assert mods.new(root, mod, ["lib/m/shared.c"]) == ["lib/m/shared.c"]
        (mod / mods.FILES / "lib/m/shared.c").write_text("void helper(void)\n{\n}\n", encoding="utf-8")
        assert mods.new(root, mod, ["lib/m/shared.c", "map/m/t01.c"]) == ["map/m/t01.c"]
        assert (mod / mods.FILES / "lib/m/shared.c").read_text(encoding="utf-8") == "void helper(void)\n{\n}\n"
        entry = mods.entries(mod)["files"]["lib/m/shared.c"]
        assert entry["original"] == tree.originals(root)["lib/m/shared.c"] and entry["unnamed"] == "lib/m/shared.c"


def test_build_layers_mods_over_the_tree_without_changing_it():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        iso_path = disc_for(folder, root)
        mod = Path(folder) / "mods" / "faster"
        mods.new(root, mod, ["lib/m/shared.c"])
        (mod / mods.FILES / "lib/m/shared.c").write_text("void helper(void)\n{\n}\n", encoding="utf-8")
        texts, clashes = mods.overlay([mod], root)
        assert clashes == {}
        found = rebuild.changes(root, iso_path, definitions.empty(), {}, natives.load(), texts)
        assert sorted(change["source"] for change in found["scripts"]) == ["map/m/t01.c", "map/m/t02.c"]
        assert (root / "lib/m/shared.c").read_text(encoding="utf-8") == LIBRARY
        assert rebuild.changes(root, iso_path, definitions.empty(), {}, natives.load())["scripts"] == []


def test_two_mods_that_change_the_same_file_cannot_be_chosen_together():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        first, second, third = (Path(folder) / "mods" / name for name in ("first", "second", "third"))
        mods.new(root, first, ["lib/m/shared.c"])
        mods.new(root, second, ["lib/m/shared.c", "map/m/t01.c"])
        mods.new(root, third, ["map/m/t02.c"])
        assert mods.overlay([first, third], root)[1] == {}
        assert mods.overlay([first, second], root)[1] == {"lib/m/shared.c": ["first", "second"]}
        listing = mods.listing(Path(folder) / "mods", root)
        assert "first  (1 file)" in listing and "lib/m/shared.c: first, second" in listing


def test_save_copies_tree_edits_into_a_mod_and_apply_writes_them_back():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        (root / "lib/m/shared.c").write_text("mine\n", encoding="utf-8")
        (root / "lib/m/extra.c").write_text("extra\n", encoding="utf-8")
        mod = Path(folder) / "mods" / "mine"
        assert mods.save(root, mod) == {"saved": ["lib/m/shared.c", "lib/m/extra.c"], "removed": []}
        fresh = recorded(Path(folder) / "fresh")
        assert mods.apply([mod], fresh)["applied"] == ["lib/m/extra.c", "lib/m/shared.c"]
        assert (fresh / "lib/m/shared.c").read_text(encoding="utf-8") == "mine\n"
        (fresh / "lib/m/shared.c").write_text("changed again\n", encoding="utf-8")
        try:
            mods.apply([mod], fresh)
            raise AssertionError("apply should refuse to replace an edit")
        except SystemExit as error:
            assert "lib/m/shared.c" in str(error)
        mods.apply([mod], fresh, force=True)
        assert (fresh / "lib/m/shared.c").read_text(encoding="utf-8") == "mine\n"


def test_a_mod_made_without_names_fits_a_named_tree():
    with tempfile.TemporaryDirectory() as folder:
        plain = recorded(Path(folder) / "plain", {"map/s/s51/s51_1.c": '#include "lib/map/s/s51/fight.c"\n',
                                                  "lib/map/s/s51/fight.c": "void fight(void)\n{\n}\n"})
        (plain / "lib/map/s/s51/lyrics.c").write_text("void lyrics(void)\n{\n}\n", encoding="utf-8")
        (plain / "map/s/s51/s51_1.c").write_text('#include "lib/map/s/s51/fight.c"\n#include "lib/map/s/s51/lyrics.c"\n',
                                                 encoding="utf-8")
        mod = Path(folder) / "mods" / "lyrics"
        mods.save(plain, mod)
        named = recorded(Path(folder) / "named", {
            "map/s/s51_raybrandt_castle/s51_1.c": '#include "lib/map/s/s51_raybrandt_castle/fight.c"\n',
            "lib/map/s/s51_raybrandt_castle/fight.c": "void fight(void)\n{\n}\n"})
        pathnames.record(named, {"map/s/s51/s51_1.c": "map/s/s51_raybrandt_castle/s51_1.c",
                                 "lib/map/s/s51/fight.c": "lib/map/s/s51_raybrandt_castle/fight.c"})
        texts = mods.contents(mod, named)
        assert sorted(texts) == ["lib/map/s/s51_raybrandt_castle/lyrics.c", "map/s/s51_raybrandt_castle/s51_1.c"]
        assert texts["map/s/s51_raybrandt_castle/s51_1.c"].split("\n") == [
            '#include "lib/map/s/s51_raybrandt_castle/fight.c"', '#include "lib/map/s/s51_raybrandt_castle/lyrics.c"', ""]


def test_a_mods_editor_flags_put_its_own_files_before_the_trees():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        mod = Path(folder) / "mods" / "faster"
        mods.write_editor_flags(mod, root, sdk.SDK_DIR, [])
        lines = (mod / mods.FILES / source.EDITOR_FLAGS).read_text(encoding="utf-8").splitlines()
        base = root.resolve().as_posix()
        assert lines[-3:] == ["-I" + base + "/include", "-I.", "-I" + base]
        assert mods.mod_files(mod) == []
