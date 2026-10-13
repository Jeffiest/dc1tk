import struct

SECTOR = 2048
PVD_SECTOR = 16


def read_at(f, offset, size):
    f.seek(offset)
    return f.read(size)


def clean_name(raw_name):
    if raw_name in (b"\x00", b"\x01"):
        return ""
    return raw_name.decode("ascii", "replace").split(";")[0].rstrip(".")


def parse_record(raw):
    name_length = raw[32]
    return {
        "name": clean_name(raw[33:33 + name_length]),
        "extent": struct.unpack_from("<I", raw, 2)[0],
        "size": struct.unpack_from("<I", raw, 10)[0],
        "is_dir": bool(raw[25] & 2),
    }


def root_record(f):
    pvd = read_at(f, PVD_SECTOR * SECTOR, SECTOR)
    if pvd[1:6] != b"CD001":
        raise ValueError("not an ISO 9660 image")
    return parse_record(pvd[156:190])


def list_directory(f, record):
    data = read_at(f, record["extent"] * SECTOR, record["size"])
    entries = []
    offset = 0
    while offset < len(data):
        length = data[offset]
        if length == 0:
            offset = (offset // SECTOR + 1) * SECTOR
            continue
        entry = parse_record(data[offset:offset + length])
        if entry["name"]:
            # where the record itself is, to update a file's extent and size
            entry["at"] = record["extent"] * SECTOR + offset
            entries.append(entry)
        offset += length
    return entries


def find(f, path):
    record = root_record(f)
    for part in path.replace("\\", "/").strip("/").split("/"):
        matches = [entry for entry in list_directory(f, record) if entry["name"].upper() == part.upper()]
        if not matches:
            return None
        record = matches[0]
    return record


def read_file(f, record):
    return read_at(f, record["extent"] * SECTOR, record["size"])


# Points a file's directory record (from find) at a new extent and size; ISO 9660 stores both
# little- and big-endian.
def set_extent(f, record, extent, size):
    f.seek(record["at"] + 2)
    f.write(struct.pack("<I", extent) + struct.pack(">I", extent) + struct.pack("<I", size) + struct.pack(">I", size))
