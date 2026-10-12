import tempfile
from pathlib import Path

from c2s import __main__ as commands
from c2s import extract, source, tree
from tests.test_extract import sample_disc


def decompiled(folder, *extra):
    extracted = Path(folder) / "extracted"
    if not extracted.is_dir():
        extract.extract(sample_disc(folder), extracted)
    out = Path(folder) / "source"
    args = commands.parser().parse_args(["decompile", str(extracted), "-o", str(out), "--defs", "none"] + list(extra))
    commands.run_decompile(args)
    return out


def test_decompiling_a_disc_records_what_it_wrote_and_writes_editor_flags():
    with tempfile.TemporaryDirectory() as folder:
        out = decompiled(folder)
        assert (out / source.EDITOR_FLAGS).is_file()
        assert "map/t01.c" in tree.originals(out)
        assert tree.status(out) == {"edited": [], "added": [], "removed": []}


def test_decompiling_again_keeps_edits_unless_forced():
    with tempfile.TemporaryDirectory() as folder:
        out = decompiled(folder)
        decompiled(folder)
        (out / "map/t01.c").write_text("// mine\n", encoding="utf-8")
        try:
            decompiled(folder)
            raise AssertionError("decompile should refuse to overwrite an edit")
        except SystemExit as error:
            assert "map/t01.c" in str(error)
        assert (out / "map/t01.c").read_text(encoding="utf-8") == "// mine\n"
        decompiled(folder, "--force")
        assert tree.status(out)["edited"] == []
