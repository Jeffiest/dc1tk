import struct

from c2s import iso

INDEX_RECORD = 12
PACK_NAME = 64
PACK_SLOT = 0x50
PACK_ALIGN = 16


def read_name(data, offset):
    end = data.index(b"\x00", offset)
    return data[offset:end].decode("cp932", "replace").replace("\\", "/").lstrip("/")


def parse_index(data):
    count = struct.unpack_from("<I", data, 0)[0] // INDEX_RECORD
    entries = []
    for i in range(count):
        name_offset, size, sector = struct.unpack_from("<iii", data, i * INDEX_RECORD)
        path = read_name(data, name_offset)
        if not path:
            break
        entries.append({"path": path, "size": size, "sector": sector})
    return entries


# The engine reads DATA.HD4 into a buffer of this size (dataread.cpp, header_buff).
INDEX_LIMIT = 0x50000


# The index with records added for new files, [(path, size, sector)], before the terminator. The
# names follow the records, so every name moves by the records added; the game's paths keep their
# order and spelling, and new ones are written with "\" as the game's are.
def with_entries(data, added):
    count = struct.unpack_from("<I", data, 0)[0] // INDEX_RECORD
    records = [struct.unpack_from("<iii", data, i * INDEX_RECORD) for i in range(count)]
    names = [data[offset:data.index(b"\x00", offset)] for offset, _, _ in records]
    rows = [(name, size, sector) for name, (_, size, sector) in zip(names, records)][:-1]
    rows += [(path.replace("/", "\\").encode("cp932"), size, sector) for path, size, sector in added]
    rows.append((b"", 0, 0))
    table, text = b"", b""
    start = len(rows) * INDEX_RECORD
    for name, size, sector in rows:
        table += struct.pack("<iii", start + len(text), size, sector)
        text += name + b"\x00"
    out = table + text
    return out + bytes(-len(out) % 4)


def open_archive(f):
    index = iso.find(f, "DATA.HD4")
    dat = iso.find(f, "DATA.DAT")
    if index is None or dat is None:
        raise ValueError("DATA.HD4 or DATA.DAT is missing from the disc")
    return {"entries": parse_index(iso.read_file(f, index)), "base": dat["extent"] * iso.SECTOR}


def read_entry(f, archive, entry):
    return iso.read_at(f, archive["base"] + entry["sector"] * iso.SECTOR, entry["size"])


def pack_name(raw):
    end = raw.find(b"\x00")
    if end <= 0:
        return None
    text = raw[:end]
    if any(byte < 0x20 or byte == 0x7F for byte in text):
        return None
    try:
        name = text.decode("cp932")
    except UnicodeDecodeError:
        return None
    if "/" in name or "\\" in name or name in (".", ".."):
        return None
    return name


def parse_pack(data):
    members = []
    offset = 0
    while offset + PACK_SLOT <= len(data):
        if data[offset] == 0:
            if offset + PACK_SLOT == len(data) and members:
                return members
            return None
        name = pack_name(data[offset:offset + PACK_NAME])
        if name is None:
            return None
        data_offset, size, next_offset = struct.unpack_from("<iii", data, offset + PACK_NAME)
        start = offset + data_offset
        if data_offset != PACK_SLOT or size < 0 or start + size > len(data):
            return None
        if next_offset != PACK_SLOT + size + (-size % PACK_ALIGN):
            return None
        members.append({"name": name, "offset": start, "size": size})
        offset += next_offset
    return None
