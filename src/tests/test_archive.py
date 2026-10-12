from c2s import archive
from tests import build


def test_parses_index_with_backslash_paths():
    index, _dat = build.archive_files([("\\map\\t01.stb", b"abc"), ("\\menu\\cfg7\\comdat.cfg", b"x" * 3000)])
    entries = archive.parse_index(index)
    assert [entry["path"] for entry in entries] == ["map/t01.stb", "menu/cfg7/comdat.cfg"]
    assert [entry["size"] for entry in entries] == [3, 3000]
    assert [entry["sector"] for entry in entries] == [0, 1]


def test_parses_pack():
    data = build.pack([("info.cfg", b"MODEL a;"), ("c01.stb", b"SB2\x00" + b"\x01" * 40)])
    members = archive.parse_pack(data)
    assert [member["name"] for member in members] == ["info.cfg", "c01.stb"]
    first = members[0]
    assert data[first["offset"]:first["offset"] + first["size"]] == b"MODEL a;"


def test_pack_needs_terminator():
    data = build.pack([("a.bin", b"1234")])
    assert [member["name"] for member in archive.parse_pack(data)] == ["a.bin"]
    assert archive.parse_pack(data[:-archive.PACK_SLOT]) is None
    assert archive.parse_pack(data + b"\x00" * 16) is None


def test_rejects_non_packs():
    assert archive.parse_pack(b"") is None
    assert archive.parse_pack(b"plain text file\n" * 20) is None
    assert archive.parse_pack(build.sample_script()) is None
    assert archive.parse_pack(b"\x00" * 200) is None
