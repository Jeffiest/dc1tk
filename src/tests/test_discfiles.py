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


def test_a_new_member_goes_at_the_end_of_its_pack():
    inner = build.pack([("tex.img", b"texture"), ("info.cfg", b"cfg")])
    outer = build.pack([("model.chr", inner)])
    added = discfiles.added_member(outer, ["model.chr", "new_motion.mot"], b"motion data")
    members = archive.parse_pack(rebuild.read_member(added, ["model.chr"]))
    assert [member["name"] for member in members] == ["tex.img", "info.cfg", "new_motion.mot"]
    assert rebuild.read_member(added, ["model.chr", "new_motion.mot"]) == b"motion data"
    assert rebuild.read_member(added, ["model.chr", "tex.img"]) == b"texture"
    # the engine matches names ignoring case, so a name only differing in case is refused
    try:
        discfiles.added_member(inner, ["TEX.IMG"], b"x")
    except ValueError as error:
        assert "other case" in str(error)
    else:
        raise AssertionError("added a member shadowed by another")


def test_new_files_get_index_records_before_the_terminator():
    index, _ = build.archive_files([("edit.cfg", b"cfg"), ("chara\\a.chr", b"model")])
    grown = archive.with_entries(index, [("chara/mod/new.chr", 5, 99)])
    entries = archive.parse_index(grown)
    assert [entry["path"] for entry in entries] == ["edit.cfg", "chara/a.chr", "chara/mod/new.chr"]
    assert entries[2]["size"] == 5 and entries[2]["sector"] == 99
    assert entries[1]["sector"] == archive.parse_index(index)[1]["sector"]
    # stored with the game's separator
    assert b"chara\\mod\\new.chr" in grown


def test_two_mods_replacing_one_game_file_clash():
    with tempfile.TemporaryDirectory() as folder:
        for name in ("a", "b"):
            path = Path(folder) / name / discfiles.DISC / "chara/x.chr.d/x.img"
            path.parent.mkdir(parents=True)
            path.write_bytes(name.encode())
        found, clashes = discfiles.collect([Path(folder) / "a", Path(folder) / "b"])
        assert found["chara/x.chr.d/x.img"][0] == "a"
        assert clashes == {"chara/x.chr.d/x.img": ["a", "b"]}
