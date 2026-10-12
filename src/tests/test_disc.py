import struct

from c2s import disc


def elf_with(table_address, records):
    header = bytearray(0x100)
    header[0:4] = b"\x7fELF"
    struct.pack_into("<I", header, 28, 0x34)
    struct.pack_into("<H", header, 44, 1)
    data = b"".join(records) + bytes(184)
    struct.pack_into("<5I", header, 0x34, 1, 0x100, table_address, table_address, len(data))
    return bytes(header) + data


def record(number, name, model):
    out = bytearray(184)
    struct.pack_into("<h", out, 0, number)
    out[4:4 + len(name)] = name.encode("ascii")
    out[36:36 + len(model)] = model.encode("ascii")
    return bytes(out)


def test_monster_table_reads_records_until_the_empty_one():
    elf = elf_with(0x1000, [record(0, "Sewer Rat", "e201a"), record(523, "Griffon Soldier", "e254a")])
    table = disc.monster_table(elf, {"address": 0x1000, "record_size": 184})
    assert table == {0: {"name": "Sewer Rat", "model": "e201a"}, 523: {"name": "Griffon Soldier", "model": "e254a"}}


def test_identifiers_are_ascii_and_repeats_get_their_number():
    assert disc.identifier("Dr.Dell", "NPC") == "NPC_DR_DELL"
    assert disc.identifier("Émile's Hat", "ITEM") == "ITEM_EMILES_HAT"
    lines = disc.enum_lines("monster_kind", "MONSTER", {37: "Griffon Soldier", 523: "Griffon Soldier"}, set())
    assert "    MONSTER_GRIFFON_SOLDIER = 37," in lines
    assert "    MONSTER_GRIFFON_SOLDIER_523 = 523," in lines


def test_header_skips_names_the_sdk_already_uses():
    text = disc.header({"item_id": {1: "Attrib"}}, {"ITEM_ATTRIB"})
    assert "ITEM_ATTRIB_1 = 1," in text
    assert disc.header({}, set()) is None
