import struct

from c2s import natives, stack, stb

MODES = {
    0x1: "local",
    0x2: "local_idx",
    0x4: "ptr_idx",
    0x8: "local_f",
    0x10: "local_idx_f",
    0x20: "ptr_idx_f",
    0x40: "global",
    0x200: "global_f",
}

COMPARES = {40: "eq", 41: "ne", 42: "lt", 43: "le", 44: "gt", 45: "ge"}

PLAIN = {
    "POP": "pop",
    "STORE": "store",
    "ADD": "add",
    "SUB": "sub",
    "MUL": "mul",
    "DIV": "div",
    "MOD": "mod",
    "NEG": "neg",
    "ITOF": "itof",
    "FTOI": "ftoi",
    "RET": "ret",
    "WAIT": "wait",
    "AND": "and",
    "OR": "or",
    "NOT": "not",
    "END": "end",
    "SKIP_END": "skip_end",
    "SIN": "sin",
    "COS": "cos",
}

COUNTED = {"PRINT": "print", "EXT": "ext", "NOP": "nop"}

JUMPS = {
    ("JMP", 0): "jmp",
    ("JMP_FALSE", 0): "jf",
    ("JMP_FALSE", 1): "jf_push",
    ("JMP_TRUE", 0): "jt",
    ("JMP_TRUE", 1): "jt_push",
}

CONST_INT = 1
CONST_FLOAT = 2
CONST_STR = 3
HEADER_PADDING = "00" * (stb.HEADER_SIZE - stb.HEADER_FIELDS_SIZE)

OPS_BY_NAME = {name: op for op, name in stb.OPCODES.items()}
MODES_BY_NAME = {name: mode for mode, name in MODES.items()}
COMPARES_BY_NAME = {name: value for value, name in COMPARES.items()}
PLAIN_BY_MNEMONIC = {mnemonic: name for name, mnemonic in PLAIN.items()}
COUNTED_BY_MNEMONIC = {mnemonic: name for name, mnemonic in COUNTED.items()}
JUMPS_BY_MNEMONIC = {mnemonic: key for key, mnemonic in JUMPS.items()}

ESCAPES = {"\\": "\\\\", '"': '\\"', "\n": "\\n", "\r": "\\r", "\t": "\\t"}
UNESCAPES = {"\\": "\\", '"': '"', "n": "\n", "r": "\r", "t": "\t"}


def quote(text):
    out = []
    for char in text:
        if char in ESCAPES:
            out.append(ESCAPES[char])
        elif ord(char) < 0x20 or ord(char) == 0x7F:
            out.append("\\x%02x" % ord(char))
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


def float_text(bits):
    raw = struct.pack("<i", bits)
    value = struct.unpack("<f", raw)[0]
    if value != value or value in (float("inf"), float("-inf")):
        return None
    for precision in range(1, 10):
        text = "%.*g" % (precision, value)
        if struct.pack("<f", float(text)) == raw:
            return repr(float(text))
    return None


def hex_text(value):
    if value < 0:
        return "-0x%x" % -value
    return "0x%x" % value


def function_label(offset):
    return "f_%04x" % offset


def jump_label(offset):
    return "L_%04x" % offset


def raw_op(item):
    return ".op %d %d %d" % (item["op"], item["arg1"], item["arg2"])


def string_offsets(region):
    offsets = []
    position = region["offset"] + stb.FUNCDATA_SIZE + len(region["instructions"]) * stb.VMCODE_SIZE
    for text in region["strings"]:
        offsets.append(position)
        position += len(text.encode("cp932")) + 1
    return offsets


def strings_in_order(region, code):
    refs = []
    for item in region["instructions"]:
        if item["name"] == "PUSH_CONST" and item["arg1"] == CONST_STR:
            refs.append(code + item["arg2"])
    return refs == string_offsets(region)


def native_positions(instructions, args):
    positions = set()
    for index, item in enumerate(instructions):
        if item["name"] == "EXT":
            position = stack.native_push(instructions, index, args)
            if position is not None:
                push = instructions[position]
                if push["name"] == "PUSH_CONST" and push["arg1"] == CONST_INT:
                    positions.add(position)
    return positions


def instruction_text(item, context):
    name = item["name"]
    op, arg1, arg2 = item["op"], item["arg1"], item["arg2"]
    if name in PLAIN:
        return PLAIN[name] if arg1 == 0 and arg2 == 0 else raw_op(item)
    if name in COUNTED:
        return "%s %d" % (COUNTED[name], arg1) if arg2 == 0 else raw_op(item)
    if name in ("LOAD", "LOAD_ADDR"):
        if arg2 not in MODES:
            return raw_op(item)
        return "%s %s %d" % ("load" if name == "LOAD" else "addr", MODES[arg2], arg1)
    if name == "CMP":
        return "cmp " + COMPARES[arg1] if arg1 in COMPARES and arg2 == 0 else raw_op(item)
    if (name, arg2) in JUMPS:
        if arg1 not in context["bounds"]:
            return raw_op(item)
        return "%s %s" % (JUMPS[(name, arg2)], jump_label(arg1))
    if name == "CALL":
        if arg1 != 0 or arg2 not in context["functions"]:
            return raw_op(item)
        return "call " + function_label(arg2)
    if name == "PUSH_CONST":
        if arg1 == CONST_INT and context["native"]:
            known = context["natives"].get(arg2)
            return "native " + (known["name"] if known else str(arg2))
        if arg1 == CONST_INT:
            return "push %d" % arg2
        if arg1 == CONST_FLOAT:
            text = float_text(arg2)
            return "push " + text if text is not None else raw_op(item)
        if arg1 == CONST_STR and context["ordered"]:
            return "push " + quote(context["strings"].pop(0))
        if arg1 == CONST_STR and arg2 in context["string_index"]:
            return "push string %d" % context["string_index"][arg2]
    return raw_op(item)


def function_text(region, code, functions, numbered, args):
    start = region["offset"] + stb.FUNCDATA_SIZE - code
    instructions = region["instructions"]
    bounds = {start + index * stb.VMCODE_SIZE for index in range(len(instructions))}
    targets = set()
    for item in instructions:
        if item["name"] in ("JMP", "JMP_FALSE", "JMP_TRUE") and item["arg1"] in bounds:
            targets.add(item["arg1"])
    header = ".function %s local %d arg %d" % (function_label(region["offset"] - code), region["local"], region["arg"])
    types = list(region["arg_types"])
    while types and types[-1] == 0:
        types.pop()
    if types:
        header += " types " + " ".join(str(value) for value in types)
    if region["name"] != 0:
        header += " name %d" % region["name"]
    lines = [header]
    context = {
        "bounds": bounds,
        "functions": functions,
        "natives": numbered,
        "ordered": strings_in_order(region, code),
        "strings": list(region["strings"]),
        "string_index": {offset - code: index for index, offset in enumerate(string_offsets(region))},
        "native": False,
    }
    pushes = native_positions(instructions, args)
    last = len(instructions) - 1
    for index, item in enumerate(instructions):
        if start + index * stb.VMCODE_SIZE in targets:
            lines.append(jump_label(start + index * stb.VMCODE_SIZE) + ":")
        if index == last and item["arg1"] == 0 and item["arg2"] == 0:
            continue
        context["native"] = index in pushes
        lines.append("    " + instruction_text(item, context))
    for text in region["strings"]:
        lines.append(".string " + quote(text))
    if region["padding"]:
        lines.append(".padding " + region["padding"])
    return lines


def magic_text(magic):
    raw = magic.encode("latin-1")
    if raw[3:] == b"\x00" and raw[:3].isalnum():
        return raw[:3].decode("ascii")
    return "0x" + raw.hex()


def header_text(fields, size, functions, programs):
    code = fields["code"]
    line = ".header " + magic_text(fields["magic"])
    if fields["main"] - code in functions:
        line += " main " + function_label(fields["main"] - code)
    else:
        line += " main " + hex_text(fields["main"])
    line += " globals %d" % fields["global_num"]
    if not stb.HEADER_SIZE <= code <= size:
        line += " code " + hex_text(code)
    if programs is None:
        line += " prog %s prog_num %d" % (hex_text(fields["prog"]), fields["prog_num"])
    if fields["unk_14"] != 0:
        line += " unk_14 %d" % fields["unk_14"]
    if fields["padding"] != HEADER_PADDING:
        line += " padding " + fields["padding"]
    return line


def disassemble(script, table, tables):
    header = script["header"]
    regions = script["regions"]
    lines = []
    if header is None:
        for region in regions:
            lines.append(".bytes " + region["hex"])
        return "\n".join(lines) + "\n"
    code = header["code"]
    if not stb.HEADER_SIZE <= code <= script["size"]:
        code_position = script["size"]
    else:
        code_position = code
    functions = {region["offset"] - code for region in regions if region["kind"] == "function"}
    programs = None
    for region in regions:
        if region["kind"] == "programs":
            programs = region
    numbered = natives.numbered(tables[table]) if table in tables else {}
    args = stack.function_args(script)
    lines.append(".natives " + table)
    code_marked = False
    for region in regions:
        if not code_marked and region["offset"] >= code_position:
            lines.append(".code")
            code_marked = True
        kind = region["kind"]
        if kind == "header":
            lines.append(header_text(header, script["size"], functions, programs))
        if kind == "programs":
            for entry in region["entries"]:
                if entry["func"] - code in functions:
                    lines.append(".program %d %s" % (entry["no"], function_label(entry["func"] - code)))
                else:
                    lines.append(".program %d %s" % (entry["no"], hex_text(entry["func"])))
        if kind == "raw":
            lines.append(".bytes " + region["hex"])
        if kind == "function":
            lines.append("")
            lines += function_text(region, code, functions, numbered, args)
    if not code_marked:
        lines.append(".code")
    return "\n".join(lines) + "\n"


def tokenize(line):
    if '"' not in line:
        return line.split()
    tokens = []
    position = 0
    while position < len(line):
        char = line[position]
        if char.isspace():
            position += 1
            continue
        if char != '"':
            end = position
            while end < len(line) and not line[end].isspace():
                end += 1
            tokens.append(line[position:end])
            position = end
            continue
        text = []
        position += 1
        while line[position] != '"':
            if line[position] == "\\":
                escape = line[position + 1]
                if escape == "x":
                    text.append(chr(int(line[position + 2:position + 4], 16)))
                    position += 4
                    continue
                text.append(UNESCAPES[escape])
                position += 2
                continue
            text.append(line[position])
            position += 1
        tokens.append({"string": "".join(text)})
        position += 1
    return tokens


def number(token):
    return int(token, 0)


def is_float(token):
    return not token.startswith(("0x", "-0x")) and ("." in token or "e" in token or "E" in token)


def float_bits(token):
    return struct.unpack("<i", struct.pack("<f", float(token)))[0]


def options(tokens):
    pairs = {}
    position = 0
    while position < len(tokens):
        key = tokens[position]
        if key == "types":
            values = []
            position += 1
            while position < len(tokens) and tokens[position].lstrip("-").isdigit():
                values.append(int(tokens[position]))
                position += 1
            pairs["types"] = values
            continue
        pairs[key] = tokens[position + 1]
        position += 2
    return pairs


def magic_bytes(token):
    if token.startswith("0x"):
        return bytes.fromhex(token[2:]).decode("latin-1")
    return (token.encode("ascii") + b"\x00").decode("latin-1")


def parse(text):
    program = {"table": None, "header": None, "items": []}
    function = None
    for number_line, line in enumerate(text.split("\n"), 1):
        tokens = tokenize(line)
        if not tokens:
            continue
        head = tokens[0]
        rest = tokens[1:]
        where = {"line": number_line}
        if head == ".natives":
            program["table"] = rest[0]
        elif head == ".header":
            settings = options(rest[1:])
            settings["magic"] = magic_bytes(rest[0])
            program["header"] = settings
            program["items"].append({"kind": "header", **where})
        elif head == ".program":
            program["items"].append({"kind": "program", "no": number(rest[0]), "target": rest[1], **where})
        elif head == ".bytes":
            program["items"].append({"kind": "raw", "hex": rest[0], **where})
        elif head == ".code":
            program["items"].append({"kind": "code", **where})
        elif head == ".function":
            settings = options(rest[1:])
            function = {"kind": "function", "label": rest[0], "settings": settings,
                        "instructions": [], "labels": {}, "strings": [], "padding": "", **where}
            program["items"].append(function)
        elif head == ".string":
            function["strings"].append(rest[0]["string"])
        elif head == ".padding":
            function["padding"] = rest[0]
        elif head.endswith(":") and len(tokens) == 1:
            function["labels"][head[:-1]] = len(function["instructions"])
        else:
            function["instructions"].append({"mnemonic": head, "operands": rest, **where})
    return program


def needs_end(function):
    instructions = function["instructions"]
    if not instructions:
        return True
    last = instructions[-1]
    return not (last["mnemonic"] == ".op" and number(last["operands"][0]) == stb.OP_FUNC_END)


def function_size(function):
    count = len(function["instructions"]) + (1 if needs_end(function) else 0)
    strings = sum(len(text.encode("cp932")) + 1 for text in function["strings"])
    return stb.FUNCDATA_SIZE + count * stb.VMCODE_SIZE + strings + len(function["padding"]) // 2


def layout(program):
    offset = 0
    code = None
    symbols = {}
    for item in program["items"]:
        item["offset"] = offset
        kind = item["kind"]
        if kind == "header":
            offset += stb.HEADER_SIZE
        if kind == "program":
            offset += stb.PROGDATA_SIZE
        if kind == "raw":
            offset += len(item["hex"]) // 2
        if kind == "code":
            code = offset
        if kind == "function":
            symbols[item["label"]] = offset
            for label, index in item["labels"].items():
                symbols[label] = offset + stb.FUNCDATA_SIZE + index * stb.VMCODE_SIZE
            offset += function_size(item)
    return {"code": code if code is not None else offset, "symbols": symbols, "size": offset}


def address(token, places, relative):
    if token in places["symbols"]:
        return places["symbols"][token] - relative
    return number(token)


def instruction(name, arg1=0, arg2=0):
    return {"op": OPS_BY_NAME[name], "name": name, "arg1": arg1, "arg2": arg2}


def encode_instruction(item, places, names, strings, queue):
    mnemonic = item["mnemonic"]
    operands = item["operands"]
    code = places["code"]
    if mnemonic == ".op":
        op, arg1, arg2 = (number(token) for token in operands)
        return {"op": op, "name": stb.OPCODES.get(op, "UNKNOWN"), "arg1": arg1, "arg2": arg2}
    if mnemonic in PLAIN_BY_MNEMONIC:
        return instruction(PLAIN_BY_MNEMONIC[mnemonic])
    if mnemonic in COUNTED_BY_MNEMONIC:
        return instruction(COUNTED_BY_MNEMONIC[mnemonic], number(operands[0]))
    if mnemonic in ("load", "addr"):
        return instruction("LOAD" if mnemonic == "load" else "LOAD_ADDR", number(operands[1]), MODES_BY_NAME[operands[0]])
    if mnemonic == "cmp":
        return instruction("CMP", COMPARES_BY_NAME[operands[0]])
    if mnemonic in JUMPS_BY_MNEMONIC:
        name, keep = JUMPS_BY_MNEMONIC[mnemonic]
        return instruction(name, address(operands[0], places, code), keep)
    if mnemonic == "call":
        return instruction("CALL", 0, address(operands[0], places, code))
    if mnemonic == "native":
        token = operands[0]
        return instruction("PUSH_CONST", CONST_INT, names[token] if token in names else number(token))
    if mnemonic == "push":
        token = operands[0]
        if isinstance(token, dict):
            return instruction("PUSH_CONST", CONST_STR, queue.pop(0) - code)
        if token == "string":
            return instruction("PUSH_CONST", CONST_STR, strings[number(operands[1])] - code)
        if is_float(token):
            return instruction("PUSH_CONST", CONST_FLOAT, float_bits(token))
        return instruction("PUSH_CONST", CONST_INT, number(token))
    raise ValueError("line %d: unknown instruction %s" % (item["line"], mnemonic))


def build_function(function, places, names):
    settings = function["settings"]
    offset = function["offset"]
    types = settings.get("types", [])
    arg_types = types + [0] * (stb.FUNCDATA_ARGS - len(types))
    code_start = offset + stb.FUNCDATA_SIZE
    strings_start = code_start + (len(function["instructions"]) + (1 if needs_end(function) else 0)) * stb.VMCODE_SIZE
    strings = []
    position = strings_start
    for text in function["strings"]:
        strings.append(position)
        position += len(text.encode("cp932")) + 1
    queue = list(strings)
    instructions = [encode_instruction(item, places, names, strings, queue) for item in function["instructions"]]
    if needs_end(function):
        instructions.append(instruction("FUNC_END"))
    return {"kind": "function", "offset": offset, "size": function_size(function),
            "addr": code_start - places["code"], "name": number(settings.get("name", "0")),
            "local": number(settings["local"]), "arg": number(settings["arg"]), "arg_types": arg_types,
            "instructions": instructions, "strings": function["strings"], "padding": function["padding"]}


def build_header(program, places):
    settings = program["header"]
    code = places["code"]
    programs = [item for item in program["items"] if item["kind"] == "program"]
    fields = {
        "magic": settings["magic"],
        "main": address(settings["main"], places, 0),
        "code": number(settings["code"]) if "code" in settings else code,
        "prog": number(settings["prog"]) if "prog" in settings else (programs[0]["offset"] if programs else 0),
        "prog_num": number(settings["prog_num"]) if "prog_num" in settings else len(programs),
        "unk_14": number(settings.get("unk_14", "0")),
        "global_num": number(settings["globals"]),
        "padding": settings.get("padding", HEADER_PADDING),
    }
    return {"kind": "header", "offset": 0, "size": stb.HEADER_SIZE, "fields": fields}


def to_script(text, tables):
    program = parse(text)
    places = layout(program)
    names = {}
    if program["table"] in tables:
        names = {native["name"]: native["no"] for native in tables[program["table"]]["natives"]}
    regions = []
    entries = []
    for item in program["items"]:
        kind = item["kind"]
        if kind == "header":
            regions.append(build_header(program, places))
        if kind == "program":
            if not entries:
                regions.append({"kind": "programs", "offset": item["offset"], "size": 0, "entries": entries})
            entries.append({"no": item["no"], "func": address(item["target"], places, 0)})
            regions[-1]["size"] += stb.PROGDATA_SIZE
        if kind == "raw":
            regions.append({"kind": "raw", "offset": item["offset"], "size": len(item["hex"]) // 2, "hex": item["hex"]})
        if kind == "function":
            regions.append(build_function(item, places, names))
    header = regions[0]["fields"] if regions and regions[0]["kind"] == "header" else None
    return {"size": places["size"], "header": header, "regions": regions}


def assemble(text, tables):
    return stb.write(to_script(text, tables))
