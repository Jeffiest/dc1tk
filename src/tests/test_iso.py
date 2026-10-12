import io

from c2s import iso
from tests import build


def sample_disc():
    return io.BytesIO(build.iso_image([("DATA.HD4", b"index"), ("DATA.DAT", b"x" * 5000)]))


def test_finds_files_case_insensitively():
    f = sample_disc()
    record = iso.find(f, "/data.hd4")
    assert record is not None
    assert iso.read_file(f, record) == b"index"


def test_reads_multi_sector_file():
    f = sample_disc()
    assert iso.read_file(f, iso.find(f, "DATA.DAT")) == b"x" * 5000


def test_missing_file_is_none():
    assert iso.find(sample_disc(), "SYSTEM.CNF") is None


def test_rejects_non_iso():
    try:
        iso.root_record(io.BytesIO(b"\x00" * 40000))
    except ValueError:
        return
    raise AssertionError("expected ValueError")
