import struct

SECTOR = 2048
ROOT_SECTOR = 18
PACK_SLOT = 0x50


def both32(value):
    return struct.pack("<I", value) + struct.pack(">I", value)


def both16(value):
    return struct.pack("<H", value) + struct.pack(">H", value)


def sectors(size):
    return max(1, (size + SECTOR - 1) // SECTOR)


def pad(data, alignment):
    return data + b"\x00" * (-len(data) % alignment)


def directory_record(name, extent, size, is_dir):
    length = 33 + len(name) + (1 if len(name) % 2 == 0 else 0)
    record = bytearray(length)
    record[0] = length
    record[2:10] = both32(extent)
    record[10:18] = both32(size)
    record[25] = 2 if is_dir else 0
    record[28:32] = both16(1)
    record[32] = len(name)
    record[33:33 + len(name)] = name
    return bytes(record)


def iso_image(files):
    layout = []
    sector = ROOT_SECTOR + 1
    for name, data in files:
        layout.append((name, sector, data))
        sector += sectors(len(data))
    root = directory_record(b"\x00", ROOT_SECTOR, SECTOR, True) + directory_record(b"\x01", ROOT_SECTOR, SECTOR, True)
    for name, start, data in layout:
        root += directory_record((name + ";1").encode("ascii"), start, len(data), False)
    image = bytearray(sector * SECTOR)
    pvd = bytearray(SECTOR)
    pvd[0] = 1
    pvd[1:6] = b"CD001"
    pvd[6] = 1
    pvd[156:190] = directory_record(b"\x00", ROOT_SECTOR, SECTOR, True)
    image[16 * SECTOR:17 * SECTOR] = pvd
    image[17 * SECTOR:17 * SECTOR + 7] = b"\xffCD001\x01"
    image[ROOT_SECTOR * SECTOR:ROOT_SECTOR * SECTOR + len(root)] = root
    for name, start, data in layout:
        image[start * SECTOR:start * SECTOR + len(data)] = data
    return bytes(image)


def archive_files(files):
    table_size = 12 * (len(files) + 1)
    names = b""
    name_offsets = []
    for path, _data in files:
        name_offsets.append(table_size + len(names))
        names += path.encode("cp932") + b"\x00"
    dat = b""
    table = b""
    for (path, data), name_offset in zip(files, name_offsets):
        table += struct.pack("<iii", name_offset, len(data), len(dat) // SECTOR)
        dat += pad(data, SECTOR)
    table += struct.pack("<iii", table_size + len(names), 0, 0)
    return table + names + b"\x00", dat


def pack(members):
    out = b""
    for name, data in members:
        body = pad(data, 16)
        header = name.encode("cp932").ljust(64, b"\x00")
        header += struct.pack("<iii", PACK_SLOT, len(data), PACK_SLOT + len(body))
        out += pad(header, 16) + body
    return out + b"\x00" * PACK_SLOT


def instruction(op, arg1=0, arg2=0):
    return struct.pack("<iii", op, arg1, arg2)


def function_record(record, code, local, arg, arg_types):
    addr = record - code + 0x38
    return struct.pack("<iiii", addr, 0, local, arg) + struct.pack("<10i", *(arg_types + [0] * (10 - len(arg_types))))


def sample_script():
    code = 0x50
    main = code
    main_code = main + 0x38
    hello = main_code + 12 * 12
    helper = hello + 16
    header = b"SB2\x00" + struct.pack("<6i", main, code, 0x40, 2, 0, 1) + b"\x00" * 0x24
    programs = struct.pack("<ii", 100, main) + struct.pack("<ii", 200, helper)
    main_body = b"".join([
        instruction(3, 3, hello - code),
        instruction(20, 1),
        instruction(3, 1, 12),
        instruction(3, 1, 5),
        instruction(19, 0, helper - code),
        instruction(2, 0, 1),
        instruction(21, 3),
        instruction(1, 0, 1),
        instruction(17, main_code + 10 * 12 - code),
        instruction(23),
        instruction(27),
        instruction(0),
    ])
    strings = b"hello\x00" + b"stale\x00item"
    helper_body = instruction(22, 0) + instruction(3, 1, 7) + instruction(15) + instruction(0)
    return (header + programs
            + function_record(main, code, 1, 0, []) + main_body + strings
            + function_record(helper, code, 1, 1, [1]) + helper_body)
