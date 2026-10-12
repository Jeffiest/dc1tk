import tempfile
from pathlib import Path

from c2s import tree
from tests.test_index import tree as make_tree

FILES = {
    "map/a.c": '#include "lib/shared.c"\nvoid a(void)\n{\n}\n',
    "map/b.c": '#include "lib/shared.c"\nvoid b(void)\n{\n}\n',
    "lib/shared.c": "void shared(void)\n{\n}\n",
}


def recorded(folder):
    root = make_tree(folder, FILES)
    tree.record(root, list(FILES))
    return root


def test_status_tells_edits_from_decompiler_output_ignoring_line_endings():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        (root / "map/a.c").write_bytes(FILES["map/a.c"].encode("utf-8").replace(b"\n", b"\r\n"))
        assert tree.status(root) == {"edited": [], "added": [], "removed": []}
        (root / "lib/shared.c").write_text("void shared(void)\n{\n    wait;\n}\n", encoding="utf-8")
        (root / "lib/new.c").write_text("", encoding="utf-8")
        (root / ".cache/clangd").mkdir(parents=True)
        (root / ".cache/clangd/index.idx").write_text("", encoding="utf-8")
        (root / "map/b.c").unlink()
        assert tree.status(root) == {"edited": ["lib/shared.c"], "added": ["lib/new.c"], "removed": ["map/b.c"]}


def test_overwrites_only_names_edited_files_or_all_of_an_unrecorded_tree():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        assert tree.overwrites(root, list(FILES)) == []
        (root / "map/a.c").write_text("edited", encoding="utf-8")
        assert tree.overwrites(root, list(FILES)) == ["map/a.c"]
        (root / tree.ORIGINALS).unlink()
        assert sorted(tree.overwrites(root, list(FILES))) == sorted(FILES)


def test_record_keeps_files_another_decompile_wrote():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        tree.record(root, ["map/a.c"])
        assert sorted(tree.originals(root)) == sorted(FILES)


def test_library_notes_count_scripts_an_edit_reaches_unasked():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        (root / "lib/shared.c").write_text("void shared(void)\n{\n    wait;\n}\n", encoding="utf-8")
        (root / "map/a.c").write_text('#include "lib/shared.c"\n// edited\n', encoding="utf-8")
        assert tree.library_notes(root, tree.status(root)) == [
            "edited library lib/shared.c is included by 2 scripts, 1 of which you did not edit"]


def test_a_saved_mod_applies_to_a_fresh_tree():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        (root / "map/a.c").write_text("mine\n", encoding="utf-8")
        (root / "lib/extra.c").write_text("extra\n", encoding="utf-8")
        mod = Path(folder) / "mod"
        assert tree.save_mod(root, mod) == {"saved": ["lib/extra.c", "map/a.c"], "removed": []}
        fresh = make_tree(Path(folder) / "fresh", FILES)
        tree.record(fresh, list(FILES))
        assert tree.apply_mod(mod, fresh)["applied"] == ["lib/extra.c", "map/a.c"]
        assert (fresh / "map/a.c").read_text(encoding="utf-8") == "mine\n"
        assert (fresh / "lib/extra.c").read_text(encoding="utf-8") == "extra\n"


def test_applying_over_an_edit_needs_force():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        (root / "map/a.c").write_text("mine\n", encoding="utf-8")
        mod = Path(folder) / "mod"
        tree.save_mod(root, mod)
        (root / "map/a.c").write_text("changed again\n", encoding="utf-8")
        try:
            tree.apply_mod(mod, root)
            raise AssertionError("apply should refuse")
        except SystemExit as error:
            assert "map/a.c" in str(error)
        tree.apply_mod(mod, root, force=True)
        assert (root / "map/a.c").read_text(encoding="utf-8") == "mine\n"


def test_fork_gives_chosen_scripts_their_own_copy():
    with tempfile.TemporaryDirectory() as folder:
        root = recorded(folder)
        result = tree.fork(root, "lib/shared.c", "lib/shared_a.c", ["map/a.c"])
        assert result == {"still_using_original": ["map/b.c"]}
        assert '#include "lib/shared_a.c"' in (root / "map/a.c").read_text(encoding="utf-8")
        assert '#include "lib/shared.c"' in (root / "map/b.c").read_text(encoding="utf-8")
        copy = (root / "lib/shared_a.c").read_text(encoding="utf-8")
        assert copy.startswith("// Copy of lib/shared.c for map/a.c") and copy.endswith(FILES["lib/shared.c"])
