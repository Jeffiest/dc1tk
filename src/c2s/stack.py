from c2s import stb

EFFECTS = {
    "FUNC_END": 0,
    "LOAD": 1,
    "LOAD_ADDR": 1,
    "PUSH_CONST": 1,
    "POP": -1,
    "STORE": -1,
    "ADD": -1,
    "SUB": -1,
    "MUL": -1,
    "DIV": -1,
    "MOD": -1,
    "NEG": 0,
    "ITOF": 0,
    "FTOI": 0,
    "CMP": -1,
    "RET": -1,
    "JMP": 0,
    "JMP_FALSE": -1,
    "JMP_TRUE": -1,
    "NOP": 0,
    "WAIT": 0,
    "AND": -1,
    "OR": -1,
    "NOT": 0,
    "END": 0,
    "SKIP_END": 0,
    "SIN": 0,
    "COS": 0,
}

CONST_INT = 1
INDEXED_MODES = (0x2, 0x4, 0x10, 0x20)


def function_args(script):
    code = script["header"]["code"]
    args = {}
    for region in script["regions"]:
        if region["kind"] == "function":
            args[region["offset"] - code] = region["arg"]
    return args


def effect(item, args):
    name = item["name"]
    if name == "CALL":
        return 1 - args.get(item["arg2"], 0)
    if name in ("LOAD", "LOAD_ADDR") and item["arg2"] in INDEXED_MODES:
        return 0
    if name in ("PRINT", "EXT"):
        return -item["arg1"]
    return EFFECTS[name]


def native_push(instructions, index, args):
    need = instructions[index]["arg1"]
    total = 0
    position = index - 1
    while position >= 0:
        total += effect(instructions[position], args)
        if total == need:
            return position
        if total > need:
            return None
        position -= 1
    return None


def is_int(item):
    return item["name"] == "PUSH_CONST" and item["arg1"] == CONST_INT


def native_calls(script):
    args = function_args(script)
    calls = []
    for region in script["regions"]:
        if region["kind"] != "function":
            continue
        instructions = region["instructions"]
        for index, item in enumerate(instructions):
            if item["name"] != "EXT":
                continue
            push = native_push(instructions, index, args)
            number = None
            if push is not None and is_int(instructions[push]):
                number = instructions[push]["arg2"]
                if is_int(instructions[push + 1]) and instructions[push + 2]["name"] == "ADD":
                    number += instructions[push + 1]["arg2"]
            calls.append({"function": region["offset"], "index": index, "args": item["arg1"] - 1, "no": number})
    return calls


def read_calls(data):
    return native_calls(stb.read(data))
