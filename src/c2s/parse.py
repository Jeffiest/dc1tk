import re

from c2s import asm

PRECEDENCE = {
    "||": 2, "&&": 3, "|": 4, "&": 5,
    "==": 6, "!=": 6, "<": 7, "<=": 7, ">": 7, ">=": 7,
    "+": 8, "-": 8, "*": 9, "/": 9, "%": 9,
}
LOGICAL = ("&&", "||")
TYPES = ("int", "float")
RETURN_TYPES = ("void", "int", "float")
NATIVE_TYPES = ("int", "float", "string", "any")
PREPROCESSOR = ("#ifndef", "#ifdef", "#if", "#else", "#elif", "#endif", "#undef")
SIMPLE = ("wait", "end", "skip_end", "break", "continue")
BUILTINS = ("sin", "cos")

NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUMBER = re.compile(r"0[xX][0-9a-fA-F]+|\d+(\.\d*)?([eE][+-]?\d+)?")
OPERATORS = ("...", "==", "!=", "<=", ">=", "&&", "||")
SINGLES = "(){}[],;=<>+-*/%&|!"
ESCAPES = {"\\": "\\", '"': '"', "n": "\n", "r": "\r", "t": "\t"}


def fail(state, reason):
    token = state["tokens"][min(state["pos"], len(state["tokens"]) - 1)]
    raise ValueError("line %d: %s" % (token["line"], reason))


def lex_fail(line, reason):
    raise ValueError("line %d: %s" % (line, reason))


def read_string(text, position, line):
    out = []
    position += 1
    while position < len(text) and text[position] != '"':
        char = text[position]
        if char == "\n":
            lex_fail(line, "string runs past the end of the line")
        if char != "\\":
            out.append(char)
            position += 1
            continue
        escape = text[position + 1:position + 2]
        if escape == "x":
            out.append(chr(int(text[position + 2:position + 4], 16)))
            position += 4
        elif escape in ESCAPES:
            out.append(ESCAPES[escape])
            position += 2
        else:
            lex_fail(line, "unknown escape \\" + escape)
    if position >= len(text):
        lex_fail(line, "unterminated string")
    return "".join(out), position + 1


def line_end(text, position):
    end = text.find("\n", position)
    return len(text) if end < 0 else end


def read_asm(text, position, line):
    opening = text.find("{", position)
    if opening < 0:
        lex_fail(line, "asm block without {")
    words = text[position:opening].split()
    closing = re.compile(r"^\}[ \t]*$", re.M).search(text, opening)
    if closing is None:
        lex_fail(line, "asm block without a closing }")
    body = text[line_end(text, opening) + 1:closing.start()]
    token = {"kind": "asm", "words": words, "lines": body.split("\n")[:-1], "line": line}
    return token, closing.end()


def comment_block(comments, line):
    block = []
    expected = line - 1
    for number, text in reversed(comments):
        if number != expected:
            break
        block.insert(0, text)
        expected -= 1
    paragraphs = [[]]
    for text in block:
        if text:
            paragraphs[-1].append(text)
        else:
            paragraphs.append([])
    return "\n".join(" ".join(paragraph) for paragraph in paragraphs if paragraph)


def tokenize(text):
    tokens = []
    comments = []
    position = 0
    line = 1
    while position < len(text):
        char = text[position]
        if char == "\n":
            line += 1
            position += 1
        elif char.isspace():
            position += 1
        elif text.startswith("//", position):
            end = line_end(text, position)
            if not text[text.rfind("\n", 0, position) + 1:position].strip():
                comments.append((line, text[position + 2:end].strip()))
            position = end
        elif text.startswith("/*", position):
            end = text.find("*/", position + 2)
            if end < 0:
                lex_fail(line, "unterminated comment")
            line += text.count("\n", position, end)
            position = end + 2
        elif text.startswith(PREPROCESSOR, position):
            position = line_end(text, position)
        elif text.startswith("#define", position):
            end = line_end(text, position)
            words = text[position + len("#define"):end].split(None, 1)
            tokens.append({"kind": "define", "words": words, "line": line})
            position = end
        elif text.startswith("#include", position):
            end = line_end(text, position)
            rest = text[position + len("#include"):end].strip()
            if rest.startswith("<") and rest.endswith(">"):
                tokens.append({"kind": "include", "path": rest[1:-1].strip(), "system": True, "line": line})
                position = end
                continue
            words = asm.tokenize(rest)
            if len(words) != 1 or not isinstance(words[0], dict):
                lex_fail(line, 'expected #include "path" or #include <path>')
            tokens.append({"kind": "include", "path": words[0]["string"], "system": False, "line": line})
            position = end
        elif text.startswith("#pragma", position):
            end = line_end(text, position)
            words = asm.tokenize(text[position + len("#pragma"):end])
            if words[:1] == ["assembly"]:
                tokens.append({"kind": "assembly", "text": text[end + 1:], "line": line})
                return tokens
            tokens.append({"kind": "pragma", "words": words, "line": line})
            position = end
        elif char == '"':
            value, position = read_string(text, position, line)
            tokens.append({"kind": "string", "text": value, "line": line})
        elif char.isdigit():
            match = NUMBER.match(text, position)
            word = match.group()
            kind = "float" if not word.lower().startswith("0x") and ("." in word or "e" in word.lower()) else "int"
            tokens.append({"kind": kind, "text": word, "line": line})
            position = match.end()
        elif NAME.match(text, position):
            match = NAME.match(text, position)
            if match.group() == "asm":
                token, end = read_asm(text, match.end(), line)
                tokens.append(token)
                line += text.count("\n", position, end)
                position = end
                continue
            token = {"kind": "name", "text": match.group(), "line": line}
            doc = comment_block(comments, line)
            if doc:
                token["doc"] = doc
            tokens.append(token)
            position = match.end()
        elif text.startswith("...", position):
            tokens.append({"kind": "op", "text": "...", "line": line})
            position += 3
        elif text[position:position + 2] in OPERATORS:
            tokens.append({"kind": "op", "text": text[position:position + 2], "line": line})
            position += 2
        elif char in SINGLES:
            tokens.append({"kind": "op", "text": char, "line": line})
            position += 1
        else:
            lex_fail(line, "unexpected character %r" % char)
    tokens.append({"kind": "eof", "text": "", "line": line})
    return tokens


def peek(state, offset=0):
    return state["tokens"][min(state["pos"] + offset, len(state["tokens"]) - 1)]


def at(state, text, offset=0):
    token = peek(state, offset)
    return token["kind"] in ("op", "name") and token["text"] == text


def take(state):
    token = peek(state)
    state["pos"] += 1
    return token


def expect(state, text):
    if not at(state, text):
        fail(state, "expected %r, found %r" % (text, peek(state)["text"]))
    return take(state)


def expect_kind(state, kind):
    if peek(state)["kind"] != kind:
        fail(state, "expected %s, found %r" % (kind, peek(state)["text"]))
    return take(state)


def int_value(text):
    if text.lower().startswith("0x"):
        return int(text, 16)
    return int(text, 10)


def literal(token):
    if token["kind"] == "int":
        return {"kind": "int", "value": int_value(token["text"])}
    return {"kind": "float", "bits": asm.float_bits(token["text"])}


def arguments(state):
    expect(state, "(")
    args = []
    while not at(state, ")"):
        args.append(assignment(state))
        if not at(state, ")"):
            expect(state, ",")
    expect(state, ")")
    return args


def primary(state):
    token = take(state)
    kind = token["kind"]
    if kind in ("int", "float"):
        return literal(token)
    if kind == "string":
        return {"kind": "string", "text": token["text"]}
    if kind == "op" and token["text"] == "(":
        node = assignment(state)
        expect(state, ")")
        return node
    if kind != "name":
        state["pos"] -= 1
        fail(state, "expected an expression, found %r" % token["text"])
    name = token["text"]
    if name in BUILTINS and at(state, "("):
        expect(state, "(")
        operand = assignment(state)
        expect(state, ")")
        return {"kind": "builtin", "name": name, "operand": operand}
    if at(state, "("):
        return {"kind": "call", "name": name, "args": arguments(state)}
    if at(state, "["):
        expect(state, "[")
        index = assignment(state)
        expect(state, "]")
        return {"kind": "index", "name": name, "index": index}
    return {"kind": "var", "name": name}


def cast_ahead(state):
    return at(state, "(") and peek(state, 1)["kind"] == "name" and peek(state, 1)["text"] in TYPES and at(state, ")", 2)


def unary(state):
    if at(state, "-") or at(state, "!"):
        op = take(state)["text"]
        return {"kind": "unary", "op": op, "operand": unary(state)}
    if at(state, "&"):
        take(state)
        target = unary(state)
        if target["kind"] not in ("var", "index", "deref"):
            fail(state, "& needs a variable")
        return {"kind": "addr", "target": target}
    if at(state, "*"):
        take(state)
        return {"kind": "deref", "operand": unary(state)}
    if cast_ahead(state):
        take(state)
        kind = take(state)["text"]
        take(state)
        return {"kind": "cast", "type": kind, "operand": unary(state)}
    return primary(state)


def binary(state, minimum):
    left = unary(state)
    while peek(state)["kind"] == "op" and PRECEDENCE.get(peek(state)["text"], 0) >= minimum:
        op = take(state)["text"]
        right = binary(state, PRECEDENCE[op] + 1)
        left = {"kind": "logical" if op in LOGICAL else "binary", "op": op, "left": left, "right": right}
    return left


def assignment(state):
    left = binary(state, 2)
    if not at(state, "="):
        return left
    if left["kind"] not in ("var", "index", "deref"):
        fail(state, "cannot assign to this expression")
    take(state)
    return {"kind": "assign", "target": left, "value": assignment(state)}


def braced(state):
    expect(state, "{")
    statements = []
    while not at(state, "}"):
        statements.append(statement(state))
    expect(state, "}")
    return statements


def conditional(state):
    expect(state, "if")
    expect(state, "(")
    cond = assignment(state)
    expect(state, ")")
    then = braced(state)
    otherwise = None
    if at(state, "else"):
        take(state)
        otherwise = [conditional(state)] if at(state, "if") else braced(state)
    return {"kind": "if", "cond": cond, "then": then, "else": otherwise}


def statement(state):
    token = peek(state)
    word = token["text"] if token["kind"] == "name" else None
    if word == "AT":
        take(state)
        expect(state, "(")
        base = int_value(expect_kind(state, "int")["text"])
        expect(state, ")")
        return dict(statement(state), at=base)
    if word == "DIRECT":
        take(state)
        return dict(statement(state), at="direct")
    if word == "if":
        return conditional(state)
    if word == "while":
        take(state)
        expect(state, "(")
        cond = assignment(state)
        expect(state, ")")
        return {"kind": "while", "cond": cond, "body": braced(state)}
    if word == "return":
        take(state)
        if at(state, ";"):
            take(state)
            return {"kind": "return", "value": None}
        value = assignment(state)
        expect(state, ";")
        return {"kind": "return", "value": value}
    if word in SIMPLE:
        take(state)
        expect(state, ";")
        return {"kind": word}
    if word == "print":
        take(state)
        args = arguments(state)
        expect(state, ";")
        return {"kind": "print", "args": args}
    node = assignment(state)
    expect(state, ";")
    return {"kind": "expr", "expr": node}


def type_name(state, allowed):
    kind = expect_kind(state, "name")["text"]
    if kind not in allowed:
        state["pos"] -= 1
        fail(state, "unknown type " + kind)
    return kind


def declarators(state, kind):
    found = []
    while True:
        name = expect_kind(state, "name")["text"]
        size = None
        if at(state, "["):
            take(state)
            size = int_value(expect_kind(state, "int")["text"])
            expect(state, "]")
        found.append({"name": name, "type": kind, "size": size})
        if not at(state, ","):
            break
        take(state)
    expect(state, ";")
    return found


def declaration_ahead(state):
    return peek(state)["kind"] == "name" and peek(state)["text"] in TYPES and peek(state, 1)["kind"] == "name"


def no_parameters(state):
    if at(state, "void") and at(state, ")", 1):
        take(state)
        return True
    return at(state, ")")


def parameters(state):
    expect(state, "(")
    params = []
    if not no_parameters(state):
        while True:
            kind = type_name(state, TYPES)
            name = expect_kind(state, "name")["text"]
            array = at(state, "[")
            if array:
                take(state)
                expect(state, "]")
            params.append({"name": name, "type": kind, "array": array})
            if not at(state, ","):
                break
            take(state)
    expect(state, ")")
    return params


def function_definition(state, pending, returns, name, doc=None):
    if name.startswith("_"):
        state["pos"] -= 1
        fail(state, "function names cannot start with _, which marks natives")
    params = parameters(state)
    expect(state, "{")
    locals_ = []
    while declaration_ahead(state):
        locals_ += declarators(state, take(state)["text"])
    body = []
    while not at(state, "}"):
        body.append(statement(state))
    expect(state, "}")
    function = {"kind": "function", "name": name, "returns": returns, "params": params, "locals": locals_, "body": body,
                "unused_strings": pending.get("strings", []), "padding": pending.get("padding")}
    if doc:
        function["doc"] = doc
    return function


def result_name(name):
    words = name.split("_")
    return "_".join(words[1:]) if len(words) > 1 else name + "_value"


def typed_parameter(state):
    enum = None
    value_kind = None
    if at(state, "enum"):
        take(state)
        enum = expect_kind(state, "name")["text"]
        kind = "int"
    elif peek(state)["kind"] == "name" and peek(state)["text"] in state.get("kinds", {}):
        value_kind = take(state)["text"]
        kind = state["kinds"][value_kind]
    else:
        kind = type_name(state, NATIVE_TYPES)
    out = at(state, "*")
    if out:
        take(state)
    found = {"name": expect_kind(state, "name")["text"], "type": kind, "out": out, "optional": False}
    if enum is not None:
        found["enum"] = enum
    if value_kind is not None:
        found["kind"] = value_kind
    return found


def native_declaration(state):
    returns = type_name(state, RETURN_TYPES + ("string", "any"))
    name = expect_kind(state, "name")["text"]
    expect(state, "(")
    params = []
    variadic = False
    if not no_parameters(state):
        while True:
            if at(state, "..."):
                take(state)
                variadic = True
            elif at(state, "OPTIONAL"):
                take(state)
                expect(state, "(")
                while True:
                    if at(state, "..."):
                        take(state)
                        variadic = True
                    else:
                        params.append(dict(typed_parameter(state), optional=True))
                    if not at(state, ","):
                        break
                    take(state)
                expect(state, ")")
            else:
                params.append(typed_parameter(state))
            if not at(state, ","):
                break
            take(state)
    expect(state, ")")
    expect(state, "NATIVE")
    expect(state, "(")
    number = int_value(expect_kind(state, "int")["text"])
    base = None
    if at(state, ","):
        take(state)
        base = number
        number += int_value(expect_kind(state, "int")["text"])
    expect(state, ")")
    expect(state, ";")
    if returns != "void":
        params.append({"name": result_name(name), "type": returns, "out": True, "optional": False, "result": True})
    return {"no": number, "base": base, "name": name, "params": params, "variadic": variadic, "returns": returns}


def inline_definition(state):
    returns = type_name(state, RETURN_TYPES)
    if returns != "void":
        fail(state, "inline helpers are statements and return void")
    name = expect_kind(state, "name")["text"]
    expect(state, "(")
    params = []
    if not no_parameters(state):
        while True:
            params.append(typed_parameter(state))
            if not at(state, ","):
                break
            take(state)
    expect(state, ")")
    return {"name": name, "params": params, "body": braced(state)}


def enum_value(state):
    negative = at(state, "-")
    if negative:
        take(state)
    value = int_value(expect_kind(state, "int")["text"])
    return -value if negative else value


def enumeration(state, program):
    enum = expect_kind(state, "name")["text"]
    expect(state, "{")
    value = 0
    while not at(state, "}"):
        name = expect_kind(state, "name")["text"]
        if at(state, "="):
            take(state)
            value = enum_value(state)
        if enum == "program":
            program["declarations"]["programs"].append({"name": name, "no": value})
        else:
            program["declarations"]["constants"].append({"name": name, "value": {"kind": "int", "value": value},
                                                         "enum": enum})
        value += 1
        if not at(state, ","):
            break
        take(state)
    expect(state, "}")
    expect(state, ";")


def definition(state, token, program):
    words = token["words"]
    if len(words) != 2:
        return
    inner = {"tokens": tokenize(words[1]), "pos": 0}
    try:
        value = unary(inner)
    except ValueError:
        return
    if peek(inner)["kind"] != "eof":
        return
    if value["kind"] == "unary" and value["op"] == "-" and value["operand"]["kind"] == "int":
        value = {"kind": "int", "value": -value["operand"]["value"]}
    if value["kind"] in ("int", "float", "string"):
        program["declarations"]["constants"].append({"name": words[0], "value": value, "enum": None})


def program_entry(state, program):
    expect(state, "(")
    token = take(state)
    if token["kind"] == "int":
        number = int_value(token["text"])
    elif token["kind"] == "name":
        number = token["text"]
    else:
        state["pos"] -= 1
        fail(state, "expected a program number or name")
    expect(state, ",")
    program["programs"].append({"no": number, "function": expect_kind(state, "name")["text"]})
    expect(state, ")")
    expect(state, ";")


def asm_function(state, token):
    words = token["words"]
    if len(words) != 3 or words[1] != "arg":
        state["pos"] -= 1
        fail(state, "asm block header must be: asm NAME arg COUNT {")
    return {"kind": "asm", "name": words[0], "arg": int(words[2]), "reason": "", "lines": token["lines"]}


def pragma(state, token, program, pending):
    words = token["words"]
    if not words:
        fail(state, "empty pragma")
    key, rest = words[0], words[1:]
    if key == "natives":
        program["natives"] = rest[0]
    elif key == "magic":
        program["magic"] = asm.magic_bytes(rest[0])
    elif key == "unk_14":
        program["unk_14"] = int(rest[0], 0)
    elif key == "header_padding":
        program["header_padding"] = rest[0]
    elif key == "globals":
        program["global_num"] = int(rest[0], 0)
    elif key == "strings":
        pending["strings"] = [word["string"] for word in rest]
    elif key == "padding":
        pending["padding"] = rest[0] if rest else ""
    elif key == "padding_of":
        program["padding_of"][rest[0]] = rest[1] if len(rest) > 1 else ""
    else:
        fail(state, "unknown pragma " + key)


def file_doc(text):
    comments = []
    lines = text.split("\n")
    for number, line in enumerate(lines, 1):
        stripped = line.strip()
        if not stripped.startswith("//"):
            break
        comments.append((number, stripped[2:].strip()))
    if not comments or (len(lines) > len(comments) and lines[len(comments)].strip()):
        return ""
    return comment_block(comments, len(comments) + 1)


def parse(text):
    state = {"tokens": tokenize(text), "pos": 0}
    if state["tokens"][0]["kind"] == "assembly":
        return {"kind": "program", "asm": state["tokens"][0]["text"]}
    program = {
        "kind": "program",
        "natives": None,
        "magic": "SB2\x00",
        "unk_14": 0,
        "header_padding": asm.HEADER_PADDING,
        "globals": [],
        "global_num": None,
        "main": None,
        "programs": [],
        "functions": [],
        "padding_of": {},
        "declarations": {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}},
    }
    doc = file_doc(text)
    if doc:
        program["doc"] = doc
    pending = {}
    while peek(state)["kind"] != "eof":
        token = take(state)
        kind = token["kind"]
        word = token["text"] if kind == "name" else None
        if kind == "pragma":
            pragma(state, token, program, pending)
        elif kind == "asm":
            program["functions"].append(asm_function(state, token))
        elif kind == "include":
            program["functions"].append({"kind": "include", "path": token["path"], "system": token["system"]})
        elif kind == "assembly":
            fail(state, "#pragma assembly must start the file")
        elif kind == "define":
            definition(state, token, program)
        elif word == "enum":
            enumeration(state, program)
        elif word == "PROGRAM":
            program_entry(state, program)
        elif word == "MAIN":
            expect(state, "(")
            program["main"] = expect_kind(state, "name")["text"]
            expect(state, ")")
            expect(state, ";")
        elif word == "typedef":
            base = type_name(state, NATIVE_TYPES)
            name = expect_kind(state, "name")["text"]
            expect(state, ";")
            program["declarations"]["kinds"][name] = base
            state.setdefault("kinds", {})[name] = base
        elif word == "extern":
            program["declarations"]["natives"].append(native_declaration(state))
        elif word == "static":
            expect(state, "inline")
            program["declarations"]["inlines"].append(inline_definition(state))
        elif word in RETURN_TYPES:
            name = expect_kind(state, "name")["text"]
            if at(state, "("):
                program["functions"].append(function_definition(state, pending, word, name, token.get("doc")))
                pending = {}
            elif word == "void":
                fail(state, "variables cannot be void")
            else:
                state["pos"] -= 1
                for declared in declarators(state, word):
                    if declared["size"] is not None:
                        fail(state, "global arrays are not supported")
                    program["globals"].append({"name": declared["name"], "type": word})
        else:
            state["pos"] -= 1
            fail(state, "expected a declaration, found %r" % token["text"])
    if program["global_num"] is None:
        program["global_num"] = len(program["globals"])
    return with_own_padding(program)


def with_own_padding(program):
    padding_of = dict(program["padding_of"])
    functions = []
    for function in program["functions"]:
        if function["kind"] == "function" and function["name"] in padding_of:
            function = dict(function, padding=padding_of.pop(function["name"]))
        functions.append(function)
    return dict(program, functions=functions, padding_of=padding_of)
