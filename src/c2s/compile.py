from c2s import asm, sdk, stb

SCALAR_MODES = {"int": "local", "float": "local_f"}
ARRAY_MODES = {"int": "local_idx", "float": "local_idx_f"}
POINTER_MODES = {"int": "ptr_idx", "float": "ptr_idx_f"}
GLOBAL_MODES = {"int": "global", "float": "global_f"}

BINARY = {"+": "add", "-": "sub", "*": "mul", "/": "div", "%": "mod", "&": "and", "|": "or"}
COMPARES = {"==": "eq", "!=": "ne", "<": "lt", "<=": "le", ">": "gt", ">=": "ge"}
UNARY = {"-": "neg", "!": "not"}
CASTS = {"float": "itof", "int": "ftoi"}
LOGICAL = {"&&": "jf_push", "||": "jt_push"}
STATEMENTS = {"wait": "wait", "end": "end", "skip_end": "skip_end"}


def fail(reason):
    raise ValueError(reason)


def global_symbols(program):
    symbols = {}
    for slot, declared in enumerate(program["globals"]):
        symbols[declared["name"]] = {"slot": slot, "type": declared["type"], "scope": "global"}
    return symbols


def frame(function, globals_):
    symbols = dict(globals_)
    slot = 0
    for param in function["params"]:
        scope = "pointer" if param["array"] else "scalar"
        symbols[param["name"]] = {"slot": slot, "type": param["type"], "scope": scope}
        slot += 1
    for declared in function["locals"]:
        scope = "scalar" if declared["size"] is None else "array"
        symbols[declared["name"]] = {"slot": slot, "type": declared["type"], "scope": scope}
        slot += 1 if declared["size"] is None else declared["size"]
    return symbols, slot


def lookup(state, name):
    if name not in state["symbols"]:
        fail("unknown variable " + name)
    return state["symbols"][name]


def scalar_mode(symbol, name):
    if symbol["scope"] == "global":
        return GLOBAL_MODES[symbol["type"]]
    if symbol["scope"] == "scalar":
        return SCALAR_MODES[symbol["type"]]
    fail(name + " is an array")


def indexed_mode(symbol, name):
    if symbol["scope"] == "array":
        return ARRAY_MODES[symbol["type"]]
    if symbol["scope"] == "pointer":
        return POINTER_MODES[symbol["type"]]
    fail(name + " is not an array")


def new_label(state):
    number = state["next_label"]
    state["next_label"] += 1
    return number


def label_name(state, number):
    return "L_%s_%d" % (state["function"], number)


def emit(state, line):
    state["lines"].append("    " + line)


def place_label(state, number):
    state["lines"].append(label_name(state, number) + ":")
    emit(state, "nop %d" % number)


def access(state, node, mnemonic):
    if node["kind"] == "var":
        symbol = lookup(state, node["name"])
        emit(state, "%s %s %d" % (mnemonic, scalar_mode(symbol, node["name"]), symbol["slot"]))
        return
    if node["kind"] == "index":
        symbol = lookup(state, node["name"])
        expression(state, node["index"])
        emit(state, "%s %s %d" % (mnemonic, indexed_mode(symbol, node["name"]), symbol["slot"]))
        return
    fail("cannot take the address of " + node["kind"])


def expression(state, node):
    kind = node["kind"]
    if kind == "int":
        emit(state, "push %d" % node["value"])
    elif kind == "float":
        emit(state, "push " + float_literal(node["bits"]))
    elif kind == "string":
        emit(state, "push string %d" % len(state["strings"]))
        state["strings"].append(node["text"])
    elif kind in ("var", "index"):
        access(state, node, "load")
    elif kind == "addr":
        access(state, node["target"], "addr")
    elif kind == "assign":
        access(state, node["target"], "addr")
        expression(state, node["value"])
        emit(state, "store")
    elif kind == "binary":
        expression(state, node["left"])
        expression(state, node["right"])
        if node["op"] in COMPARES:
            emit(state, "cmp " + COMPARES[node["op"]])
        else:
            emit(state, BINARY[node["op"]])
    elif kind == "unary":
        expression(state, node["operand"])
        emit(state, UNARY[node["op"]])
    elif kind == "cast":
        expression(state, node["operand"])
        emit(state, CASTS[node["type"]])
    elif kind == "builtin":
        expression(state, node["operand"])
        emit(state, node["name"])
    elif kind == "call":
        if node["name"] not in state["functions"]:
            fail("unknown function " + node["name"])
        if len(node["args"]) != state["functions"][node["name"]]:
            fail("%s takes %d arguments" % (node["name"], state["functions"][node["name"]]))
        for arg in node["args"]:
            expression(state, arg)
        emit(state, "call " + node["name"])
    elif kind == "logical":
        expression(state, node["left"])
        label = new_label(state)
        emit(state, "%s %s" % (LOGICAL[node["op"]], label_name(state, label)))
        expression(state, node["right"])
        place_label(state, label)
    else:
        fail("unknown expression " + kind)


def float_literal(bits):
    text = asm.float_text(bits)
    if text is None:
        fail("float constant without a decimal form")
    return text


def native_number(state, name):
    if name in state["natives"]:
        return state["natives"][name]
    if name.startswith("_EXT_") and name[5:].isdigit():
        return int(name[5:])
    fail("unknown native " + name)


def statement(state, node):
    kind = node["kind"]
    if kind == "expr":
        expression(state, node["expr"])
        emit(state, "pop")
    elif kind == "return":
        if node["value"] is None:
            emit(state, "push 0")
        else:
            expression(state, node["value"])
        emit(state, "ret")
    elif kind == "native":
        number = native_number(state, node["name"])
        if node["base"] is None:
            emit(state, "push %d" % number)
        else:
            emit(state, "push %d" % node["base"])
            emit(state, "push %d" % (number - node["base"]))
            emit(state, "add")
        for arg in node["args"]:
            expression(state, arg)
        emit(state, "ext %d" % (len(node["args"]) + 1))
    elif kind == "print":
        for arg in node["args"]:
            expression(state, arg)
        emit(state, "print %d" % len(node["args"]))
    elif kind in STATEMENTS:
        emit(state, STATEMENTS[kind])
    elif kind == "if":
        conditional(state, node)
    elif kind == "while":
        loop(state, node)
    elif kind in ("break", "continue"):
        if not state["loops"]:
            fail(kind + " outside a loop")
        emit(state, "jmp " + label_name(state, state["loops"][-1][kind]))
    else:
        fail("unknown statement " + kind)


def block(state, statements):
    for node in statements:
        statement(state, node)


def conditional(state, node):
    expression(state, node["cond"])
    otherwise = new_label(state)
    emit(state, "jf " + label_name(state, otherwise))
    block(state, node["then"])
    if node["else"] is None:
        place_label(state, otherwise)
        return
    end = new_label(state)
    emit(state, "jmp " + label_name(state, end))
    place_label(state, otherwise)
    block(state, node["else"])
    place_label(state, end)


def loop(state, node):
    top = new_label(state)
    end = new_label(state)
    place_label(state, top)
    expression(state, node["cond"])
    emit(state, "jf " + label_name(state, end))
    state["loops"].append({"continue": top, "break": end})
    block(state, node["body"])
    state["loops"].pop()
    emit(state, "jmp " + label_name(state, top))
    place_label(state, end)


def arg_types(params):
    types = [0 if param["array"] else 1 for param in params[:10]]
    while types and types[-1] == 0:
        types.pop()
    return types


def function_lines(function, context):
    symbols, size = frame(function, context["globals"])
    header = ".function %s local %d arg %d" % (function["name"], size, len(function["params"]))
    types = arg_types(function["params"])
    if types:
        header += " types " + " ".join(str(value) for value in types)
    state = {
        "function": function["name"],
        "symbols": symbols,
        "functions": context["functions"],
        "natives": context["natives"],
        "lines": [header],
        "strings": list(function["unused_strings"]),
        "next_label": 0,
        "loops": [],
    }
    block(state, function["body"])
    emit(state, "push 0")
    emit(state, "ret")
    for text in state["strings"]:
        state["lines"].append(".string " + asm.quote(text))
    padding, buffer = stb.block_padding(context["buffer"], state["strings"])
    context["buffer"] = buffer
    if function["padding"] is not None:
        padding = function["padding"]
    if padding:
        state["lines"].append(".padding " + padding)
    return state["lines"]


def asm_strings(lines):
    strings = []
    for line in lines:
        tokens = asm.tokenize(line)
        if tokens and tokens[0] == ".string":
            strings.append(tokens[1]["string"])
    return strings


def header_line(program):
    line = ".header %s main %s globals %d" % (asm.magic_text(program["magic"]), program["main"], program["global_num"])
    if program["unk_14"] != 0:
        line += " unk_14 %d" % program["unk_14"]
    if program["header_padding"] != asm.HEADER_PADDING:
        line += " padding " + program["header_padding"]
    return line


def to_asm(program, tables):
    if "asm" in program:
        return program["asm"]
    if program["main"] is None:
        fail("no main function declared")
    if any(function["kind"] == "include" for function in program["functions"]):
        fail("includes must be expanded before compiling")
    table = tables.get(program["natives"], {"natives": []})
    context = {
        "globals": global_symbols(program),
        "functions": {function["name"]: function["arg"] if function["kind"] == "asm" else len(function["params"])
                      for function in program["functions"]},
        "natives": {native["name"]: native["no"] for native in table["natives"]},
        "buffer": b"",
    }
    lines = [".natives " + program["natives"], header_line(program)]
    for entry in program["programs"]:
        lines.append(".program %d %s" % (entry["no"], entry["function"]))
    lines.append(".code")
    for function in program["functions"]:
        lines.append("")
        if function["kind"] == "asm":
            context["buffer"] = stb.block_padding(context["buffer"], asm_strings(function["lines"]))[1]
            lines += function["lines"]
        else:
            lines += function_lines(function, context)
    return "\n".join(lines) + "\n"


def compile_program(program, tables):
    return asm.assemble(to_asm(sdk.resolve(program, tables), tables), tables)
