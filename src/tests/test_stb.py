import random

from c2s import stb
from tests import build


def kinds(script):
    return [region["kind"] for region in script["regions"]]


def functions(script):
    return [region for region in script["regions"] if region["kind"] == "function"]


def test_sample_round_trips_and_is_fully_parsed():
    data = build.sample_script()
    script = stb.read(data)
    assert stb.write(script) == data
    assert stb.coverage(script) == 1.0
    assert kinds(script) == ["header", "programs", "function", "function"]


def test_sample_fields():
    script = stb.read(build.sample_script())
    assert script["header"]["magic"] == "SB2\x00"
    assert script["header"]["global_num"] == 1
    programs = script["regions"][1]
    assert [entry["no"] for entry in programs["entries"]] == [100, 200]
    main, helper = functions(script)
    assert [item["name"] for item in main["instructions"]][-3:] == ["WAIT", "END", "FUNC_END"]
    assert main["strings"] == ["hello"]
    assert main["padding"] == b"stale\x00item".hex()
    assert helper["arg"] == 1
    assert helper["arg_types"][:2] == [1, 0]
    assert helper["instructions"][0]["name"] == "NOP"


def test_edit_changes_output():
    script = stb.read(build.sample_script())
    script["regions"][1]["entries"][0]["no"] = 101
    assert stb.write(script) != build.sample_script()


def test_header_padding_survives():
    data = bytearray(build.sample_script())
    data[0x1C:0x40] = b"\xcc" * 0x24
    data[0:4] = b"STB\x00"
    script = stb.read(bytes(data))
    assert stb.write(script) == bytes(data)
    assert stb.coverage(script) == 1.0


def test_trailing_bytes_join_last_function():
    data = build.sample_script() + b"\x00" * 12
    script = stb.read(data)
    assert stb.write(script) == data
    assert stb.coverage(script) == 1.0
    assert functions(script)[-1]["padding"] == "00" * 12


def test_unknown_opcode_falls_back_to_raw():
    data = bytearray(build.sample_script())
    helper = functions(stb.read(bytes(data)))[1]
    data[helper["offset"] + stb.FUNCDATA_SIZE] = 99
    script = stb.read(bytes(data))
    assert stb.write(script) == bytes(data)
    assert kinds(script)[-1] == "raw"
    assert stb.coverage(script) < 1.0


def test_unknown_bytes_survive():
    data = bytes(random.Random(7).randrange(256) for _ in range(3000))
    assert stb.write(stb.read(data)) == data


def test_short_and_empty_files_survive():
    for data in (b"", b"SB2", b"SB2\x00" + b"\xff" * 30, b"SB2\x00" + b"\xff" * 80):
        assert stb.write(stb.read(data)) == data
