from c2s import asm, fingerprint, natives, stack, stb

SCALAR_MODES = {0x1: "int", 0x8: "float"}
ARRAY_MODES = {0x2: "int", 0x10: "float"}
POINTER_MODES = {0x4: "int", 0x20: "float"}
GLOBAL_MODES = {0x40: "int", 0x200: "float"}
INDEXED_MODES = (0x2, 0x4, 0x10, 0x20)

BINARY = {"ADD": "+", "SUB": "-", "MUL": "*", "DIV": "/", "MOD": "%", "AND": "&", "OR": "|"}
UNARY = {"NEG": "-", "NOT": "!"}
CASTS = {"ITOF": "float", "FTOI": "int"}
BUILTINS = {"SIN": "sin", "COS": "cos"}
STATEMENTS = {"WAIT": "wait", "END": "end", "SKIP_END": "skip_end"}
LOGICAL = {"JMP_FALSE": "&&", "JMP_TRUE": "||"}

CONST_INT = 1
CONST_FLOAT = 2
CONST_STR = 3
MAX_GLOBALS = 0x10000


def fail(reason):
    raise ValueError(reason)


def function_name(offset):
    return "f_%04x" % offset


def slot_modes(region):
    modes = {}
    for item in region["instructions"]:
        if item["name"] in ("LOAD", "LOAD_ADDR") and item["arg2"] not in GLOBAL_MODES:
            modes.setdefault(item["arg1"], set()).add(item["arg2"])
    for slot, found in modes.items():
        if len(found) != 1:
            fail("slot %d used with modes %s" % (slot, sorted(found)))
    return {slot: found.pop() for slot, found in modes.items()}


def declarations(region):
    modes = slot_modes(region)
    params = []
    for slot in range(region["arg"]):
        mode = modes.get(slot)
        declared = region["arg_types"][slot] if slot < stb.FUNCDATA_ARGS else 1
        if mode in POINTER_MODES:
            params.append({"name": "a%d" % slot, "type": POINTER_MODES[mode], "array": True})
        elif mode in SCALAR_MODES or mode is None:
            params.append({"name": "a%d" % slot, "type": SCALAR_MODES.get(mode, "int"), "array": mode is None and declared == 0})
        else:
            fail("parameter %d uses mode %d" % (slot, mode))
    locals_ = []
    slot = region["arg"]
    while slot < region["local"]:
        mode = modes.get(slot)
        if mode in ARRAY_MODES:
            end = slot + 1
            while end < region["local"] and end not in modes:
                end += 1
            locals_.append({"name": "v%d" % slot, "type": ARRAY_MODES[mode], "size": end - slot})
            slot = end
            continue
        if mode is not None and mode not in SCALAR_MODES:
            fail("local %d uses mode %d" % (slot, mode))
        locals_.append({"name": "v%d" % slot, "type": SCALAR_MODES.get(mode, "int"), "size": None})
        slot += 1
    for found in modes:
        if found >= region["local"] or found < 0:
            fail("slot %d outside the frame" % found)
    return params, locals_


def arg_types_for(params):
    types = [0 if param["array"] else 1 for param in params[:stb.FUNCDATA_ARGS]]
    return types + [0] * (stb.FUNCDATA_ARGS - len(types))


def variable_name(slot, mode, region):
    if mode in GLOBAL_MODES:
        return "g%d" % slot
    if slot < region["arg"]:
        return "a%d" % slot
    return "v%d" % slot


def target(state, index):
    item = state["ins"][index]
    offset = item["arg1"]
    position = (offset - state["start"]) // stb.VMCODE_SIZE
    if (offset - state["start"]) % stb.VMCODE_SIZE or not 0 <= position < len(state["ins"]):
        fail("jump into the middle of an instruction")
    if state["ins"][position]["name"] != "NOP":
        fail("jump to an instruction that is not a label")
    return position


def place(item, state):
    mode = item["arg2"]
    name = variable_name(item["arg1"], mode, state["region"])
    return {"kind": "var", "name": name}


def pop(values, count):
    if len(values) < count:
        fail("stack underflow")
    taken = values[len(values) - count:]
    del values[len(values) - count:]
    return taken


def expression(state, index, stop):
    ins = state["ins"]
    values = []
    while index < stop:
        item = ins[index]
        name = item["name"]
        if name in ("LOAD", "LOAD_ADDR"):
            node = place(item, state)
            if item["arg2"] in INDEXED_MODES:
                node = {"kind": "index", "name": node["name"], "index": pop(values, 1)[0]}
            if name == "LOAD_ADDR":
                node = {"kind": "addr", "target": node}
            values.append(node)
        elif name == "PUSH_CONST":
            values.append(constant(item, index, state))
        elif name == "STORE":
            destination, value = pop(values, 2)
            if destination["kind"] != "addr":
                fail("store through a computed address")
            values.append({"kind": "assign", "target": destination["target"], "value": value})
        elif name in BINARY:
            left, right = pop(values, 2)
            values.append({"kind": "binary", "op": BINARY[name], "left": left, "right": right})
        elif name == "CMP":
            if item["arg1"] not in asm.COMPARES:
                fail("unknown comparison")
            left, right = pop(values, 2)
            op = {"eq": "==", "ne": "!=", "lt": "<", "le": "<=", "gt": ">", "ge": ">="}[asm.COMPARES[item["arg1"]]]
            values.append({"kind": "binary", "op": op, "left": left, "right": right})
        elif name in UNARY:
            values.append({"kind": "unary", "op": UNARY[name], "operand": pop(values, 1)[0]})
        elif name in CASTS:
            values.append({"kind": "cast", "type": CASTS[name], "operand": pop(values, 1)[0]})
        elif name in BUILTINS:
            values.append({"kind": "builtin", "name": BUILTINS[name], "operand": pop(values, 1)[0]})
        elif name == "CALL":
            callee = item["arg2"]
            if callee not in state["functions"]:
                fail("call to something that is not a function")
            args = pop(values, state["functions"][callee])
            values.append({"kind": "call", "name": function_name(callee), "args": args})
        elif name in LOGICAL and item["arg2"] == 1:
            left = pop(values, 1)[0]
            label_index = target(state, index)
            if label_index <= index or label_index >= stop:
                fail("short-circuit jump leaves its expression")
            inner, end, terminator = expression(state, index + 1, label_index)
            if terminator is not None or len(inner) != 1:
                fail("short-circuit operand is not one expression")
            values.append({"kind": "logical", "op": LOGICAL[name], "left": left, "right": inner[0],
                           "label": ins[label_index]["arg1"]})
            index = label_index + 1
            continue
        else:
            return values, index, item
        index += 1
    return values, index, None


def constant(item, index, state):
    kind = item["arg1"]
    if kind in (CONST_INT, CONST_FLOAT) and item["arg2"] < 0 and index not in state["natives"]:
        fail("negative constant")
    if kind == CONST_INT:
        if index in state["natives"]:
            return {"kind": "native_number", "value": item["arg2"]}
        return {"kind": "int", "value": item["arg2"]}
    if kind == CONST_FLOAT:
        if asm.float_text(item["arg2"]) is None:
            fail("float constant without a decimal form")
        return {"kind": "float", "bits": item["arg2"]}
    if kind == CONST_STR:
        if not state["strings"]:
            fail("string pushes do not follow the string block")
        return {"kind": "string", "text": state["strings"].pop(0)}
    fail("unknown constant type")


def native_name(number, state):
    known = state["native_table"].get(number)
    if known is not None:
        return known["name"]
    return "_EXT_%d" % number


def simple_statement(values, item, state):
    name = item["name"]
    if name == "POP":
        if len(values) != 1:
            fail("expression statement leaves values")
        return {"kind": "expr", "expr": values[0]}
    if name == "RET":
        if len(values) != 1:
            fail("return leaves values")
        return {"kind": "return", "value": values[0]}
    if name == "EXT":
        if len(values) != item["arg1"] or item["arg2"] != 0:
            fail("native call inside an expression")
        number = values[0]
        if number["kind"] == "native_number":
            return {"kind": "native", "name": native_name(number["value"], state), "base": None, "args": values[1:]}
        if (number["kind"] == "binary" and number["op"] == "+" and number["left"]["kind"] == "native_number"
                and number["right"]["kind"] == "int"):
            base = number["left"]["value"]
            return {"kind": "native", "name": native_name(base + number["right"]["value"], state), "base": base,
                    "args": values[1:]}
        fail("native number is not a constant")
    if name == "PRINT":
        if len(values) != item["arg1"] or item["arg2"] != 0:
            fail("print inside an expression")
        return {"kind": "print", "args": values}
    if name in STATEMENTS:
        if values or item["arg1"] != 0 or item["arg2"] != 0:
            fail("statement with operands")
        return {"kind": STATEMENTS[name]}
    fail("unexpected " + name)


def block(state, index, stop, loops):
    statements = []
    ins = state["ins"]
    while index < stop:
        if ins[index]["name"] == "NOP":
            statement, index = loop(state, index, stop, loops)
            statements.append(statement)
            continue
        values, index, item = expression(state, index, stop)
        if item is None:
            fail("expression runs into the end of a block")
        name = item["name"]
        if name == "JMP_FALSE" and item["arg2"] == 0:
            if len(values) != 1:
                fail("condition leaves values")
            statement, index = conditional(state, values[0], index, stop, loops)
            statements.append(statement)
            continue
        if name == "JMP":
            if values or item["arg2"] != 0 or not loops:
                fail("jump outside a loop")
            label = ins[target(state, index)]["arg1"]
            if label == loops[-1]["end"]:
                statements.append({"kind": "break"})
            elif label == loops[-1]["top"]:
                statements.append({"kind": "continue"})
            else:
                fail("jump that is neither break nor continue")
            index += 1
            continue
        statements.append(simple_statement(values, item, state))
        index += 1
    return statements


def conditional(state, condition, index, stop, loops):
    ins = state["ins"]
    else_index = target(state, index)
    if not index < else_index < stop:
        fail("if jumps outside its block")
    before = ins[else_index - 1]
    if before["name"] == "JMP" and before["arg2"] == 0 and else_index - 1 > index:
        end_index = target(state, else_index - 1)
        if else_index < end_index < stop:
            statement = {
                "kind": "if",
                "cond": condition,
                "then": block(state, index + 1, else_index - 1, loops),
                "else": block(state, else_index + 1, end_index, loops),
                "labels": [ins[else_index]["arg1"], ins[end_index]["arg1"]],
            }
            return statement, end_index + 1
    statement = {
        "kind": "if",
        "cond": condition,
        "then": block(state, index + 1, else_index, loops),
        "else": None,
        "labels": [ins[else_index]["arg1"]],
    }
    return statement, else_index + 1


def loop(state, index, stop, loops):
    ins = state["ins"]
    top = ins[index]["arg1"]
    values, jump, item = expression(state, index + 1, stop)
    if item is None or item["name"] != "JMP_FALSE" or item["arg2"] != 0 or len(values) != 1:
        fail("label that does not start a loop")
    end_index = target(state, jump)
    if not jump < end_index < stop:
        fail("loop exit outside its block")
    back = ins[end_index - 1]
    if back["name"] != "JMP" or back["arg2"] != 0 or target(state, end_index - 1) != index:
        fail("loop without a jump back to its top")
    end = ins[end_index]["arg1"]
    body = block(state, jump + 1, end_index - 1, loops + [{"top": top, "end": end}])
    return {"kind": "while", "cond": values[0], "body": body, "labels": [top, end]}, end_index + 1


def expression_labels(node, out):
    kind = node["kind"]
    if kind == "logical":
        expression_labels(node["left"], out)
        out.append(node["label"])
        expression_labels(node["right"], out)
    elif kind == "binary":
        expression_labels(node["left"], out)
        expression_labels(node["right"], out)
    elif kind in ("unary", "cast", "builtin"):
        expression_labels(node["operand"], out)
    elif kind == "assign":
        expression_labels(node["target"], out)
        expression_labels(node["value"], out)
    elif kind == "index":
        expression_labels(node["index"], out)
    elif kind == "addr":
        expression_labels(node["target"], out)
    elif kind == "call":
        for arg in node["args"]:
            expression_labels(arg, out)


def statement_labels(statements, out):
    for statement in statements:
        kind = statement["kind"]
        if kind in ("expr",):
            expression_labels(statement["expr"], out)
        elif kind == "return":
            expression_labels(statement["value"], out)
        elif kind in ("native", "print"):
            for arg in statement["args"]:
                expression_labels(arg, out)
        elif kind == "if":
            expression_labels(statement["cond"], out)
            out.append(statement["labels"][0])
            statement_labels(statement["then"], out)
            if statement["else"] is not None:
                out.append(statement["labels"][1])
                statement_labels(statement["else"], out)
        elif kind == "while":
            out.append(statement["labels"][0])
            out.append(statement["labels"][1])
            expression_labels(statement["cond"], out)
            statement_labels(statement["body"], out)
    return out


def strip_labels(node):
    if isinstance(node, list):
        return [strip_labels(item) for item in node]
    if not isinstance(node, dict):
        return node
    return {key: strip_labels(value) for key, value in node.items() if key != "labels" and key != "label"}


def unused_strings(region, code):
    refs = [code + item["arg2"] for item in region["instructions"]
            if item["name"] == "PUSH_CONST" and item["arg1"] == CONST_STR]
    offsets = asm.string_offsets(region)
    skipped = len(offsets) - len(refs)
    if skipped < 0 or offsets[skipped:] != refs:
        fail("string pushes do not follow the string block")
    return region["strings"][:skipped]


def structured(region, context):
    instructions = region["instructions"]
    if len(instructions) < 3:
        fail("function too short")
    tail = instructions[-3:]
    if not (tail[0]["name"] == "PUSH_CONST" and tail[0]["arg1"] == CONST_INT and tail[0]["arg2"] == 0
            and tail[1]["name"] == "RET" and tail[1]["arg1"] == 0 and tail[1]["arg2"] == 0):
        fail("function does not end in return 0")
    if region["name"] != 0:
        fail("function record has a name")
    code = context["code"]
    unused = unused_strings(region, code)
    params, locals_ = declarations(region)
    if arg_types_for(params) != region["arg_types"]:
        fail("argument types differ from the declarations")
    state = {
        "ins": instructions,
        "region": region,
        "start": region["offset"] + stb.FUNCDATA_SIZE - code,
        "functions": context["functions"],
        "natives": asm.native_positions(instructions, context["functions"]),
        "native_table": context["native_table"],
        "strings": region["strings"][len(unused):],
    }
    body = block(state, 0, len(instructions) - 3, [])
    if state["strings"]:
        fail("unused strings in the string block")
    labels = statement_labels(body, [])
    if labels != list(range(len(labels))):
        fail("labels are not numbered in allocation order")
    types = dict(context["global_types"])
    types.update({declared["name"]: declared["type"] for declared in params + locals_})
    name = function_name(region["offset"] - code)
    returns = return_type(body, types, context["returns"])
    context["returns"][name] = returns
    return {
        "kind": "function",
        "name": name,
        "returns": returns,
        "params": params,
        "locals": locals_,
        "body": strip_labels(body),
        "unused_strings": unused,
        "padding": region["padding"],
    }


COMPARISONS = ("==", "!=", "<", "<=", ">", ">=")


def value_type(node, types, returns):
    kind = node["kind"]
    if kind in ("int", "float", "string"):
        return kind
    if kind in ("var", "index"):
        return types.get(node["name"], "int")
    if kind == "assign":
        return value_type(node["target"], types, returns)
    if kind == "binary":
        if node["op"] in COMPARISONS or node["op"] in ("&", "|"):
            return "int"
        sides = {value_type(node["left"], types, returns), value_type(node["right"], types, returns)}
        return "float" if "float" in sides else "int"
    if kind == "unary":
        return value_type(node["operand"], types, returns) if node["op"] == "-" else "int"
    if kind == "cast":
        return node["type"]
    if kind == "builtin":
        return "float"
    if kind == "call":
        found = returns.get(node["name"], "int")
        return "int" if found == "void" else found
    return "int"


def returned_values(statements, out):
    for node in statements:
        if node["kind"] == "return":
            out.append(node["value"])
        elif node["kind"] == "if":
            returned_values(node["then"], out)
            returned_values(node["else"] or [], out)
        elif node["kind"] == "while":
            returned_values(node["body"], out)
    return out


def return_type(body, types, returns):
    values = returned_values(body, [])
    if not values:
        return "void"
    if any(value_type(value, types, returns) == "float" for value in values):
        return "float"
    return "int"


def valued_calls(node, out):
    if isinstance(node, list):
        for item in node:
            valued_calls(item, out)
    elif isinstance(node, dict):
        if node.get("kind") == "expr" and node["expr"]["kind"] == "call":
            valued_calls(node["expr"]["args"], out)
            return out
        if node.get("kind") == "call":
            out.append(node["name"])
        for value in node.values():
            valued_calls(value, out)
    return out


def fallback(region, context, reason):
    lines = asm.function_text(region, context["code"], set(context["functions"]), context["native_table"], context["functions"])
    return {"kind": "asm", "name": function_name(region["offset"] - context["code"]), "arg": region["arg"],
            "reason": reason, "lines": lines}


def global_types(script):
    types = {}
    for region in script["regions"]:
        if region["kind"] == "function":
            for item in region["instructions"]:
                if item["name"] in ("LOAD", "LOAD_ADDR") and item["arg2"] in GLOBAL_MODES:
                    types.setdefault(item["arg1"], set()).add(GLOBAL_MODES[item["arg2"]])
    return types


def regular(script):
    header = script["header"]
    if header is None:
        return False
    kinds = [region["kind"] for region in script["regions"]]
    if "raw" in kinds or kinds[:2] != ["header", "programs"]:
        return False
    if header["prog"] != stb.HEADER_SIZE or header["code"] != stb.HEADER_SIZE + header["prog_num"] * stb.PROGDATA_SIZE:
        return False
    functions = {region["offset"] for region in script["regions"] if region["kind"] == "function"}
    if header["main"] not in functions:
        return False
    for entry in script["regions"][1]["entries"]:
        if entry["func"] not in functions:
            return False
    return True


def decompile(script, table, tables):
    native_table = natives.numbered(tables[table]) if table in tables else {}
    if not regular(script):
        return {"kind": "program", "asm": asm.disassemble(script, table, tables)}
    header = script["header"]
    code = header["code"]
    context = {"code": code, "functions": stack.function_args(script), "native_table": native_table,
               "global_types": {}, "returns": {}}
    found = global_types(script)
    if found and not 0 <= min(found) <= max(found) < MAX_GLOBALS:
        return {"kind": "program", "asm": asm.disassemble(script, table, tables)}
    globals_ = []
    for slot in range(max(found) + 1 if found else 0):
        types = found.get(slot, {"int"})
        if len(types) != 1:
            return {"kind": "program", "asm": asm.disassemble(script, table, tables)}
        globals_.append({"name": "g%d" % slot, "type": types.pop()})
    context["global_types"] = {declared["name"]: declared["type"] for declared in globals_}
    functions = []
    buffer = b""
    for region in script["regions"][2:]:
        predicted, buffer = stb.block_padding(buffer, region["strings"])
        try:
            function = structured(region, context)
            if function["padding"] == predicted:
                function["padding"] = None
            functions.append(function)
        except ValueError as error:
            functions.append(fallback(region, context, str(error)))
    prints = fingerprint.both_fingerprints(script, table)
    used = valued_calls([function.get("body", []) for function in functions], [])
    functions = [dict(function, returns="int") if function.get("returns") == "void" and function["name"] in used else function
                 for function in functions]
    return {
        "kind": "program",
        "natives": table,
        "magic": header["magic"],
        "unk_14": header["unk_14"],
        "header_padding": header["padding"],
        "globals": globals_,
        "global_num": header["global_num"],
        "main": function_name(header["main"] - code),
        "programs": [{"no": entry["no"], "function": function_name(entry["func"] - code)}
                     for entry in script["regions"][1]["entries"]],
        "functions": functions,
        "padding_of": {},
        "declarations": {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}},
        "fingerprints": {function_name(offset): found for offset, found in prints["exact"].items()},
        "loose_fingerprints": {function_name(offset): found for offset, found in prints["loose"].items()},
    }
