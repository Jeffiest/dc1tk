from c2s import asm, sdk, stb

# A string variable uses the int modes: they move a value without changing its type, and the VM
# keeps each value's type with it.
SCALAR_MODES = {"int": "local", "float": "local_f", "string": "local"}
ARRAY_MODES = {"int": "local_idx", "float": "local_idx_f", "string": "local_idx"}
POINTER_MODES = {"int": "ptr_idx", "float": "ptr_idx_f", "string": "ptr_idx"}
GLOBAL_MODES = {"int": "global", "float": "global_f", "string": "global"}

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
        scope = "pointer" if declared.get("pointer") else "scalar" if declared["size"] is None else "array"
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
    if symbol["scope"] == "pointer":
        # The slot holds the pointer itself; an int mode moves it without retyping it as a float.
        return "local"
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
    if node["kind"] == "deref":
        operand = node["operand"]
        if operand["kind"] != "var":
            fail("* works on a pointer or array variable; write p[i] for the others")
        symbol = lookup(state, operand["name"])
        emit(state, "push 0")
        emit(state, "%s %s %d" % (mnemonic, indexed_mode(symbol, operand["name"]), symbol["slot"]))
        return
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
    elif kind == "var" and lookup(state, node["name"])["scope"] == "array":
        # An array used as a value is the address of its first element, as in C.
        symbol = lookup(state, node["name"])
        emit(state, "push 0")
        emit(state, "addr %s %d" % (ARRAY_MODES[symbol["type"]], symbol["slot"]))
    elif kind in ("var", "index", "deref"):
        access(state, node, "load")
    elif kind == "addr":
        access(state, node["target"], "addr")
    elif kind == "assign":
        access(state, node["target"], "addr")
        expression(state, node["value"])
        emit(state, "store")
    elif kind == "binary" and node["op"] == "^":
        exclusive_or(state, node)
    elif kind == "binary" and node["op"] in ("<<", ">>"):
        shift(state, node)
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
    elif kind == "compound":
        compound(state, node)
    elif kind == "via":
        emit(state, "push 0")
        emit(state, "load %s %d" % (POINTER_MODES[node["type"]], node["slot"]))
    elif kind == "ternary":
        expression(state, node["cond"])
        otherwise = new_label(state)
        end = new_label(state)
        emit(state, "jf " + label_name(state, otherwise))
        expression(state, node["then"])
        emit(state, "jmp " + label_name(state, end))
        place_label(state, otherwise)
        expression(state, node["else"])
        place_label(state, end)
    else:
        fail("unknown expression " + kind)


# target op= value. A variable compiles as target = target op value. An element's address is worked
# out once, into a hidden local, and the element is read and written through it, so an index with
# side effects (a[i++] += 1) runs them once.
def compound(state, node):
    target = node["target"]
    if target["kind"] == "var":
        access(state, target, "addr")
        expression(state, {"kind": "binary", "op": node["op"], "left": target, "right": node["value"]})
        emit(state, "store")
        return
    name = target["name"] if target["kind"] == "index" else target["operand"].get("name")
    symbol = lookup(state, name)
    slot = temporary(state)
    emit(state, "addr local %d" % slot)
    access(state, target, "addr")
    emit(state, "store")
    emit(state, "pop")
    emit(state, "push 0")
    emit(state, "addr ptr_idx %d" % slot)
    via = {"kind": "via", "slot": slot, "type": symbol["type"]}
    expression(state, {"kind": "binary", "op": node["op"], "left": via, "right": node["value"]})
    emit(state, "store")


# A hidden local for the compiler's own use, past the function's declared locals.
def temporary(state):
    slot = state["next_slot"]
    state["next_slot"] += 1
    return slot


def store_temporary(state, slot, node):
    emit(state, "addr local %d" % slot)
    expression(state, node)
    emit(state, "store")
    emit(state, "pop")


# a ^ b, which the VM has no instruction for, as (a | b) - (a & b), with a and b worked out once.
def exclusive_or(state, node):
    left, right = temporary(state), temporary(state)
    store_temporary(state, left, node["left"])
    store_temporary(state, right, node["right"])
    for op in ("or", "and"):
        emit(state, "load local %d" % left)
        emit(state, "load local %d" % right)
        emit(state, op)
    emit(state, "sub")


# a << n and a >> n as multiplying or dividing by 2 to the n. The VM's division rounds toward
# zero, so >> of a negative number rounds up where an arithmetic shift rounds down.
def shift(state, node):
    op = "mul" if node["op"] == "<<" else "div"
    count = node["right"]
    if count["kind"] == "int":
        expression(state, node["left"])
        emit(state, "push %d" % (1 << count["value"]))
        emit(state, op)
        return
    value, times = temporary(state), temporary(state)
    store_temporary(state, value, node["left"])
    store_temporary(state, times, count)
    top, end = new_label(state), new_label(state)
    place_label(state, top)
    emit(state, "load local %d" % times)
    emit(state, "push 0")
    emit(state, "cmp gt")
    emit(state, "jf " + label_name(state, end))
    for slot, step in ((value, ["push 2", op]), (times, ["push 1", "sub"])):
        emit(state, "addr local %d" % slot)
        emit(state, "load local %d" % slot)
        for line in step:
            emit(state, line)
        emit(state, "store")
        emit(state, "pop")
    emit(state, "jmp " + label_name(state, top))
    place_label(state, end)
    emit(state, "load local %d" % value)


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
    if node.get("note") and state.get("notes") is not None:
        # a line note, at the index of the statement's first instruction
        state["notes"][str(sum(1 for line in state["lines"][1:] if line.startswith("    ")))] = node["note"]
    try:
        statement_code(state, node)
    except ValueError as error:
        if node.get("line") is None or str(error).startswith("line "):
            raise
        raise ValueError("line %d: %s" % (node["line"], error)) from None


def statement_code(state, node):
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
    elif kind == "switch":
        switch(state, node)
    elif kind in ("break", "continue"):
        targets = [entry[kind] for entry in reversed(state["loops"]) if entry[kind] is not None]
        if not targets:
            fail(kind + (" outside a loop" if kind == "continue" else " outside a loop or switch"))
        emit(state, "jmp " + label_name(state, targets[0]))
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


# The value is worked out once into a hidden local, then compared with each case in order; cases
# fall through into the next as in C, and break leaves the switch.
def switch(state, node):
    slot = temporary(state)
    store_temporary(state, slot, node["value"])
    labels = [new_label(state) for _ in node["cases"]]
    end = new_label(state)
    fallback = end
    for case, label in zip(node["cases"], labels):
        if case["default"]:
            fallback = label
            continue
        emit(state, "load local %d" % slot)
        expression(state, case["value"])
        emit(state, "cmp eq")
        emit(state, "jt " + label_name(state, label))
    emit(state, "jmp " + label_name(state, fallback))
    state["loops"].append({"continue": None, "break": end})
    for case, label in zip(node["cases"], labels):
        place_label(state, label)
        block(state, case["body"])
    state["loops"].pop()
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
        # in a pool file the block starts with the file's strings so far (stb.pool_block)
        "strings": (stb.split_strings(context["buffer"]) if context.get("pool") else []) + list(function["unused_strings"]),
        "next_label": 0,
        "next_slot": size,
        "loops": [],
        "notes": {} if context.get("notes") is not None else None,
    }
    block(state, function["body"])
    if state["notes"]:
        context["notes"][function["name"]] = state["notes"]
    emit(state, "push 0")
    emit(state, "ret")
    if state["next_slot"] != size:
        state["lines"][0] = header.replace(" local %d " % size, " local %d " % state["next_slot"], 1)
    if context.get("pool"):
        prefix = len(stb.split_strings(context["buffer"]))
        _, padding, buffer = stb.pool_block(context["buffer"], state["strings"][prefix:])
        if prefix == len(state["strings"]):
            # a function that pushes nothing writes the pool as padding, not as strings
            state["strings"] = []
    for text in state["strings"]:
        state["lines"].append(".string " + asm.quote(text))
    if not context.get("pool"):
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


# The bytes an assembly function's block holds, which is the pool after it in a pool file.
def asm_block(lines):
    padding = ""
    for line in lines:
        tokens = asm.tokenize(line)
        if tokens and tokens[0] == ".padding":
            padding = tokens[1]
    return b"".join(stb.encode_string(text) for text in asm_strings(lines)) + bytes.fromhex(padding)


def header_line(program):
    line = ".header %s main %s globals %d" % (asm.magic_text(program["magic"]), program["main"], program["global_num"])
    if program["unk_14"] != 0:
        line += " unk_14 %d" % program["unk_14"]
    if program["header_padding"] != asm.HEADER_PADDING:
        line += " padding " + program["header_padding"]
    return line


def to_asm(program, tables, notes=None):
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
        "pool": program.get("string_pool", False),
        "notes": notes,
    }
    for entry in program["programs"]:
        if entry["function"] not in context["functions"]:
            fail("PROGRAM(%s, %s): no function %s" % (entry["no"], entry["function"], entry["function"]))
    if program["main"] not in context["functions"]:
        fail("MAIN(%s): no function %s" % (program["main"], program["main"]))
    lines = [".natives " + program["natives"], header_line(program)]
    for entry in program["programs"]:
        lines.append(".program %d %s" % (entry["no"], entry["function"]))
    lines.append(".code")
    for function in program["functions"]:
        lines.append("")
        if function["kind"] == "asm":
            if context["pool"]:
                context["buffer"] = asm_block(function["lines"])
            else:
                context["buffer"] = stb.block_padding(context["buffer"], asm_strings(function["lines"]))[1]
            lines += function["lines"]
        else:
            lines += function_lines(function, context)
    # the code section ends zero-filled to 16, unless the last block was given in full
    last = next((function for function in reversed(program["functions"]) if function["kind"] in ("function", "asm")), None)
    if last is not None and last["kind"] == "function" and last["padding"] is None:
        lines.append(".tail")
    return "\n".join(lines) + "\n"


# notes, when given, collects each function's line notes: {function: {instruction index: text}}.
def compile_program(program, tables, notes=None):
    return asm.assemble(to_asm(sdk.resolve(program, tables), tables, notes), tables)
