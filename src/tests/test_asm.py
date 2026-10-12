import random
import struct

from c2s import asm, natives, stb
from tests import build


def round_trip(data, table="event"):
    tables = natives.load()
    text = asm.disassemble(stb.read(data), table, tables)
    return asm.assemble(text, tables), text


def test_sample_round_trips_symbolically():
    data = build.sample_script()
    rebuilt, text = round_trip(data)
    assert rebuilt == data
    assert ".header SB2 main f_0000 globals 1" in text
    assert "    native _ASSIGN_STACK" in text
    assert '    push "hello"' in text
    assert "    jf L_00b0" in text
    assert "    call f_00d8" in text
    assert "    addr local 0" in text
    assert ".function f_00d8 local 1 arg 1 types 1" in text
    assert ".op" not in text
    assert "func_end" not in text


def test_assembly_is_editable():
    text = round_trip(build.sample_script())[1].replace("push 5", "push 6")
    script = stb.read(asm.assemble(text, natives.load()))
    main = script["regions"][2]
    assert main["instructions"][3]["arg2"] == 6


def test_unusual_operands_fall_back_to_raw_ops():
    data = bytearray(build.sample_script())
    main = stb.read(bytes(data))["regions"][2]
    wait = main["offset"] + stb.FUNCDATA_SIZE + 9 * stb.VMCODE_SIZE
    struct.pack_into("<i", data, wait + 4, 3)
    rebuilt, text = round_trip(bytes(data))
    assert rebuilt == bytes(data)
    assert "    .op 23 3 0" in text


def test_header_variants_round_trip():
    data = bytearray(build.sample_script())
    data[0:4] = b"STB\x00"
    data[0x1C:0x40] = b"\xcc" * 0x24
    struct.pack_into("<i", data, 0x14, 7)
    rebuilt, text = round_trip(bytes(data))
    assert rebuilt == bytes(data)
    assert "unk_14 7" in text
    assert "padding " + "cc" * 0x24 in text


def test_float_text_is_shortest_exact():
    def bits(value):
        return struct.unpack("<i", struct.pack("<f", value))[0]
    assert asm.float_text(bits(0.1)) == "0.1"
    assert asm.float_text(bits(1.0)) == "1.0"
    assert asm.float_text(bits(-0.0)) == "-0.0"
    assert asm.float_text(bits(1e20)) == "1e+20"
    assert asm.float_text(0x7FC00000) is None


def test_quoted_strings_survive_tokenizing():
    text = 'a\\b "c"\n\t\x01 立ち'
    assert asm.tokenize("push " + asm.quote(text)) == ["push", {"string": text}]


def test_arbitrary_bytes_round_trip():
    generator = random.Random(3)
    samples = [b"", b"SB2", bytes(generator.randrange(256) for _ in range(500))]
    for _ in range(20):
        data = bytearray(build.sample_script())
        for _ in range(4):
            data[generator.randrange(len(data))] = generator.randrange(256)
        samples.append(bytes(data))
    for data in samples:
        assert round_trip(data)[0] == data
