import struct

HEADER_SIZE = 0x40
HEADER_FIELDS_SIZE = 0x1C
HEADER_WORDS = ("main", "code", "prog", "prog_num", "unk_14", "global_num")
PROGDATA_SIZE = 8
FUNCDATA_SIZE = 0x38
FUNCDATA_ARGS = 10
VMCODE_SIZE = 12

OPCODES = {
    0: "FUNC_END",
    1: "LOAD",
    2: "LOAD_ADDR",
    3: "PUSH_CONST",
    4: "POP",
    5: "STORE",
    6: "ADD",
    7: "SUB",
    8: "MUL",
    9: "DIV",
    10: "MOD",
    11: "NEG",
    12: "ITOF",
    13: "FTOI",
    14: "CMP",
    15: "RET",
    16: "JMP",
    17: "JMP_FALSE",
    18: "JMP_TRUE",
    19: "CALL",
    20: "PRINT",
    21: "EXT",
    22: "NOP",
    23: "WAIT",
    24: "AND",
    25: "OR",
    26: "NOT",
    27: "END",
    28: "SKIP_END",
    29: "SIN",
    30: "COS",
}

OP_FUNC_END = 0
OP_PUSH_CONST = 3
CONST_STR = 3


def word(data, offset):
    return struct.unpack_from("<i", data, offset)[0]


def read_header(data):
    fields = {"magic": data[0:4].decode("latin-1")}
    for i, name in enumerate(HEADER_WORDS):
        fields[name] = word(data, 4 + i * 4)
    fields["padding"] = data[HEADER_FIELDS_SIZE:HEADER_SIZE].hex()
    return {"kind": "header", "offset": 0, "size": HEADER_SIZE, "fields": fields}


def read_programs(data, header):
    start = header["prog"]
    count = header["prog_num"]
    end = start + count * PROGDATA_SIZE
    if count <= 0 or start < HEADER_SIZE or end > header["code"] or end > len(data):
        return None
    entries = []
    for i in range(count):
        offset = start + i * PROGDATA_SIZE
        entries.append({"no": word(data, offset), "func": word(data, offset + 4)})
    return {"kind": "programs", "offset": start, "size": end - start, "entries": entries}


def is_function(data, code, offset):
    if offset % 4 != 0 or offset + FUNCDATA_SIZE > len(data):
        return False
    return word(data, offset) == offset - code + FUNCDATA_SIZE


def next_function(data, code, offset):
    position = offset + (-offset % 4)
    while position < len(data) and not is_function(data, code, position):
        position += 4
    return min(position, len(data))


def read_instructions(data, start):
    instructions = []
    position = start
    while position + VMCODE_SIZE <= len(data):
        op, arg1, arg2 = struct.unpack_from("<iii", data, position)
        if op not in OPCODES:
            return None
        instructions.append({"op": op, "name": OPCODES[op], "arg1": arg1, "arg2": arg2})
        position += VMCODE_SIZE
        if op == OP_FUNC_END:
            return instructions
    return None


def string_refs(code, instructions):
    refs = []
    for item in instructions:
        if item["op"] == OP_PUSH_CONST and item["arg1"] == CONST_STR:
            refs.append(code + item["arg2"])
    return refs


def read_strings(data, start, end, refs):
    used = start
    for ref in refs:
        if start <= ref < end:
            terminator = data.find(b"\x00", ref, end)
            if terminator < 0:
                return None
            used = max(used, terminator + 1)
    strings = []
    position = start
    while position < used:
        terminator = data.index(b"\x00", position, used)
        raw = data[position:terminator]
        try:
            text = raw.decode("cp932")
        except UnicodeDecodeError:
            return None
        if text.encode("cp932") != raw:
            return None
        strings.append(text)
        position = terminator + 1
    return {"strings": strings, "padding": data[used:end].hex()}


def read_function(data, code, offset):
    addr, name, local, arg = struct.unpack_from("<iiii", data, offset)
    arg_types = list(struct.unpack_from("<%di" % FUNCDATA_ARGS, data, offset + 16))
    instructions = read_instructions(data, offset + FUNCDATA_SIZE)
    if instructions is None:
        return None
    strings_start = offset + FUNCDATA_SIZE + len(instructions) * VMCODE_SIZE
    end = next_function(data, code, strings_start)
    block = read_strings(data, strings_start, end, string_refs(code, instructions))
    if block is None:
        return None
    return {"kind": "function", "offset": offset, "size": end - offset,
            "addr": addr, "name": name, "local": local, "arg": arg, "arg_types": arg_types,
            "instructions": instructions, "strings": block["strings"], "padding": block["padding"]}


def raw(data, start, end):
    return {"kind": "raw", "offset": start, "size": end - start, "hex": data[start:end].hex()}


def read_code(data, code):
    regions = []
    position = code
    while position < len(data):
        if is_function(data, code, position):
            region = read_function(data, code, position)
            if region is not None:
                regions.append(region)
                position += region["size"]
                continue
            end = next_function(data, code, position + 1)
        else:
            end = next_function(data, code, position)
        regions.append(raw(data, position, end))
        position = end
    return regions


def read(data):
    if len(data) < HEADER_SIZE:
        return {"size": len(data), "header": None, "regions": [raw(data, 0, len(data))] if data else []}
    header = read_header(data)
    fields = header["fields"]
    regions = [header]
    position = HEADER_SIZE
    code = fields["code"]
    if code < HEADER_SIZE or code > len(data):
        code = len(data)
    programs = read_programs(data, fields)
    if programs is not None:
        if programs["offset"] > position:
            regions.append(raw(data, position, programs["offset"]))
        regions.append(programs)
        position = programs["offset"] + programs["size"]
    if code > position:
        regions.append(raw(data, position, code))
    regions += read_code(data, code)
    return {"size": len(data), "header": fields, "regions": regions}


def encode_string(text):
    return text.encode("cp932") + b"\x00"


# The original compiler kept one string buffer per file, zero-filled at the start. A function wrote
# its pushed strings into it, rounded the write offset up to 16 and wrote the buffer up to there
# after FUNC_END. Most files reset the offset for each function (block_padding: the strings, then
# whatever the buffer held up to the next 16); some never reset it, so every function writes the
# whole pool of the file's strings so far with its own after them (pool_block). After the last
# function the code section is zero-filled to a multiple of 16 (tail_padding).
def block_padding(previous, strings):
    raw = b"".join(encode_string(text) for text in strings)
    if not raw:
        return "", previous
    end = len(raw) + (-len(raw) % 16)
    buffer = raw + previous[len(raw):]
    buffer += bytes(max(0, end - len(buffer)))
    return buffer[len(raw):end].hex(), buffer


def split_strings(data):
    out, position = [], 0
    while position < len(data):
        terminator = data.index(b"\x00", position)
        out.append(data[position:terminator].decode("cp932"))
        position = terminator + 1
    return out


# A function's block in a file whose string pool is never reset: (the pool's strings, which come
# before its own in the block, the padding after them, the pool after the function). A function
# that pushes nothing writes the pool as padding, as the reader splits a block.
def pool_block(pool, strings):
    raw = b"".join(encode_string(text) for text in strings)
    if not raw:
        return [], pool.hex(), pool
    block = pool + raw
    end = len(block) + (-len(block) % 16)
    return split_strings(pool), "00" * (end - len(block)), block + bytes(end - len(block))


def tail_padding(end, code):
    return "00" * (-(end - code) % 16)


def encode(region):
    kind = region["kind"]
    if kind == "header":
        fields = region["fields"]
        words = struct.pack("<6i", *(fields[name] for name in HEADER_WORDS))
        return fields["magic"].encode("latin-1") + words + bytes.fromhex(fields["padding"])
    if kind == "programs":
        return b"".join(struct.pack("<ii", entry["no"], entry["func"]) for entry in region["entries"])
    if kind == "function":
        out = struct.pack("<iiii", region["addr"], region["name"], region["local"], region["arg"])
        out += struct.pack("<%di" % FUNCDATA_ARGS, *region["arg_types"])
        out += b"".join(struct.pack("<iii", item["op"], item["arg1"], item["arg2"]) for item in region["instructions"])
        out += b"".join(encode_string(text) for text in region["strings"])
        return out + bytes.fromhex(region["padding"])
    if kind == "raw":
        return bytes.fromhex(region["hex"])
    raise ValueError("unknown region kind " + kind)


def write(script):
    out = bytearray()
    for region in script["regions"]:
        if region["offset"] != len(out):
            raise ValueError("region at 0x%x does not follow 0x%x" % (region["offset"], len(out)))
        out += encode(region)
    return bytes(out)


def coverage(script):
    if script["size"] == 0:
        return 1.0
    unknown = sum(region["size"] for region in script["regions"] if region["kind"] == "raw")
    return 1 - unknown / script["size"]
