import tempfile
from pathlib import Path

from c2s import archive, discfiles, rebuild
from tests import build


def test_disc_paths_name_files_and_pack_members():
    assert discfiles.location("snd2/sp/sp_000.snd") == ("snd2/sp/sp_000.snd", [])
    assert discfiles.location("chara/c01_base.chr.d/c01b.img") == ("chara/c01_base.chr", ["c01b.img"])
    assert discfiles.location("a/b.pac.d/c.chr.d/d.img") == ("a/b.pac", ["c.chr", "d.img"])


def test_a_member_is_replaced_inside_its_pack_and_the_rest_kept():
    inner = build.pack([("tex.img", b"old texture"), ("info.cfg", b"cfg")])
    outer = build.pack([("model.chr", inner), ("other.bin", b"other")])
    new = b"a longer texture than before, to move the members after it"
    replaced = rebuild.replaced_member(outer, ["model.chr", "tex.img"], new)
    assert rebuild.read_member(replaced, ["model.chr", "tex.img"]) == new
    assert rebuild.read_member(replaced, ["model.chr", "info.cfg"]) == b"cfg"
    assert rebuild.read_member(replaced, ["other.bin"]) == b"other"
    assert archive.parse_pack(replaced) is not None


def test_two_mods_replacing_one_game_file_clash():
    with tempfile.TemporaryDirectory() as folder:
        for name in ("a", "b"):
            path = Path(folder) / name / discfiles.DISC / "chara/x.chr.d/x.img"
            path.parent.mkdir(parents=True)
            path.write_bytes(name.encode())
        found, clashes = discfiles.collect([Path(folder) / "a", Path(folder) / "b"])
        assert found["chara/x.chr.d/x.img"][0] == "a"
        assert clashes == {"chara/x.chr.d/x.img": ["a", "b"]}
