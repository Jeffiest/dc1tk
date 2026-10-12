import json
import tempfile
from pathlib import Path

from c2s import extract
from tests import build


def sample_disc(folder):
    script = build.sample_script()
    chara = build.pack([("info.cfg", b"MODEL c01;"), ("c01.stb", script)])
    index, dat = build.archive_files([
        ("\\map\\t01.stb", script),
        ("\\chara\\c01.chr", chara),
        ("\\readme.txt", b"hello"),
    ])
    path = Path(folder) / "disc.iso"
    path.write_bytes(build.iso_image([("DATA.HD4", index), ("DATA.DAT", dat)]))
    return path


def test_extracts_archive_and_packs():
    with tempfile.TemporaryDirectory() as folder:
        out = Path(folder) / "out"
        manifest = extract.extract(sample_disc(folder), out)
        assert (out / "map/t01.stb").read_bytes() == build.sample_script()
        assert (out / "chara/c01.chr.d/c01.stb").read_bytes() == build.sample_script()
        assert (out / "chara/c01.chr.d/info.cfg").read_bytes() == b"MODEL c01;"
        assert (out / "readme.txt").read_bytes() == b"hello"
        assert manifest["scripts"] == ["map/t01.stb", "chara/c01.chr.d/c01.stb"]
        assert json.loads((out / "manifest.json").read_text()) == manifest


def test_manifest_records_sizes_and_packing():
    with tempfile.TemporaryDirectory() as folder:
        manifest = extract.extract(sample_disc(folder), Path(folder) / "out")
        sizes = {item["path"]: (item["size"], item["packed"]) for item in manifest["files"]}
        assert sizes["readme.txt"] == (5, False)
        assert sizes["chara/c01.chr.d/info.cfg"] == (10, True)
