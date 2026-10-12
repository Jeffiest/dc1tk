import tempfile
from pathlib import Path

from c2s import archive, compile, definitions, natives, parse, rebuild
from tests import build

SCRIPT = "#pragma natives event\n\nPROGRAM(100, f);\nMAIN(f);\n\nvoid f(void)\n{\n%s}\n"


def script(body):
    return compile.compile_program(parse.parse(SCRIPT % body), natives.load())


def disc(folder, filler):
    chara = build.pack([("info.cfg", b"MODEL c01;"), ("c01.stb", script("    wait;\n"))])
    index, dat = build.archive_files([
        ("\\map\\t01.stb", script("    wait;\n")),
        ("\\map\\t02.stb", script("    end;\n")),
        ("\\chara\\c01.chr", chara),
    ])
    files = [("DATA.HD4", index), ("DATA.DAT", dat)] + ([("DMMY", bytes(filler * build.SECTOR))] if filler else [])
    path = Path(folder) / "disc.iso"
    path.write_bytes(build.iso_image(files))
    return path


def source(folder, scripts):
    root = Path(folder) / "source"
    for path, body in scripts.items():
        (root / path).parent.mkdir(parents=True, exist_ok=True)
        (root / path).write_text(SCRIPT % body, encoding="utf-8")
    return root


def built(folder, filler, scripts):
    iso_path = disc(folder, filler)
    out = Path(folder) / "out.iso"
    found = rebuild.changes(source(folder, scripts), iso_path, definitions.empty(), {}, natives.load())
    result = rebuild.write_disc(iso_path, out, found["files"])
    return iso_path, out, found, result


def read(path, top, members):
    with open(path, "rb") as f:
        disc_index = archive.open_archive(f)
        entry = next(entry for entry in disc_index["entries"] if entry["path"] == top)
        return rebuild.read_member(archive.read_entry(f, disc_index, entry), members)


def test_unchanged_source_rebuilds_the_same_image():
    with tempfile.TemporaryDirectory() as folder:
        iso_path, out, found, _ = built(folder, 4, {"map/t01.c": "    wait;\n", "map/t02.c": "    end;\n"})
        assert found["scripts"] == []
        assert out.read_bytes() == iso_path.read_bytes()


def test_changed_scripts_land_in_place_inside_packs_and_in_the_filler():
    with tempfile.TemporaryDirectory() as folder:
        long_body = "    wait;\n" * 400
        _, out, found, result = built(folder, 4, {"map/t01.c": long_body, "map/t02.c": "    end;\n",
                                                  "chara/c01.chr.d/c01.c": "    wait;\n    wait;\n"})
        assert sorted(change["source"] for change in found["scripts"]) == ["chara/c01.chr.d/c01.c", "map/t01.c"]
        assert {item["path"]: item["moved"] for item in result["placed"]} == {"chara/c01.chr": False, "map/t01.stb": True}
        assert result["grown"] == 0
        assert read(out, "map/t01.stb", []) == script(long_body)
        assert read(out, "map/t02.stb", []) == script("    end;\n")
        assert read(out, "chara/c01.chr", ["c01.stb"]) == script("    wait;\n    wait;\n")
        assert read(out, "chara/c01.chr", ["info.cfg"]) == b"MODEL c01;"
        assert rebuild.check(out, found["scripts"]) == []


def test_image_grows_when_there_is_no_filler():
    with tempfile.TemporaryDirectory() as folder:
        iso_path, out, found, result = built(folder, 0, {"map/t01.c": "    wait;\n" * 400})
        assert result["grown"] > 0
        assert out.stat().st_size == iso_path.stat().st_size + result["grown"] * build.SECTOR
        assert read(out, "map/t01.stb", []) == script("    wait;\n" * 400)
