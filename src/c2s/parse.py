import ast
import re

from c2s import asm

PRECEDENCE = {
    "||": 2, "&&": 3, "|": 4, "^": 5, "&": 6,
    "==": 7, "!=": 7, "<": 8, "<=": 8, ">": 8, ">=": 8,
    "<<": 9, ">>": 9,
    "+": 10, "-": 10, "*": 11, "/": 11, "%": 11,
}
LOGICAL = ("&&", "||")
TYPES = ("int", "float", "string")
CAST_TYPES = ("int", "float")
RETURN_TYPES = ("void", "int", "float", "string")
NATIVE_TYPES = ("int", "float", "string", "any")
CONDITIONALS = ("if", "ifdef", "ifndef", "elif", "else", "endif")
SIMPLE = ("wait", "end", "skip_end", "break", "continue")
BUILTINS = ("sin", "cos")
COMPOUND = {"+=": "+", "-=": "-", "*=": "*", "/=": "/", "%=": "%", "&=": "&", "|=": "|", "^=": "^",
            "<<=": "<<", ">>=": ">>"}

NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
NUMBER = re.compile(r"(0[xX][0-9a-fA-F]+|\d+(\.\d*)?([eE][+-]?\d+)?)([fFuUlL]*)")
DIRECTIVE = re.compile(r"#[ \t]*([A-Za-z_]+)[ \t]*(.*)")
LONG_OPERATORS = ("...", "<<=", ">>=")
OPERATORS = ("==", "!=", "<=", ">=", "&&", "||", "<<", ">>", "++", "--",
             "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=")
SINGLES = "(){}[],;=<>+-*/%&|!^~?:"
ESCAPES = {"\\": "\\", '"': '"', "'": "'", "n": "\n", "r": "\r", "t": "\t", "0": "\0"}


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


# A character literal, 'a' or '\n', as its character code.
def read_char(text, position, line):
    if text[position + 1:position + 2] == "\\":
        escape = text[position + 2:position + 3]
        if escape not in ESCAPES:
            lex_fail(line, "unknown escape \\" + escape)
        value, position = ESCAPES[escape], position + 3
    else:
        value, position = text[position + 1:position + 2], position + 2
    if not value or text[position:position + 1] != "'":
        lex_fail(line, "a character literal holds one character, as in 'a'")
    return ord(value), position + 1


# The value of an #if or #elif condition: integers, defined(NAME), names #defined to integers (any
# other name is 0, as in C), ! && || ?, comparisons and arithmetic.
def condition_value(text, defines, line):
    expression = text.split("//")[0]
    expression = re.sub(r"defined\s*\(\s*(\w+)\s*\)|defined\s+(\w+)",
                        lambda match: "1" if (match.group(1) or match.group(2)) in defines else "0", expression)
    for _ in range(16):
        replaced = re.sub(r"\b[A-Za-z_]\w*\b", lambda match: "(%s)" % (defines.get(match.group()) or "0"), expression)
        if replaced == expression:
            break
        expression = replaced
    expression = re.sub(r"\b(0[xX][0-9a-fA-F]+|\d+)[uUlL]+", r"\1", expression)
    expression = expression.replace("&&", " and ").replace("||", " or ")
    expression = re.sub(r"!(?!=)", " not ", expression)
    try:
        return bool(folded_condition(ast.parse(expression.strip() or "0", mode="eval").body))
    except (SyntaxError, ValueError, ZeroDivisionError):
        lex_fail(line, "cannot read the condition %r" % text.strip())


def folded_condition(node):
    if isinstance(node, ast.Constant) and isinstance(node.value, int):
        return node.value
    if isinstance(node, ast.BoolOp):
        values = [folded_condition(value) for value in node.values]
        return int(all(values)) if isinstance(node.op, ast.And) else int(any(values))
    if isinstance(node, ast.UnaryOp):
        operand = folded_condition(node.operand)
        return {ast.Not: lambda: int(not operand), ast.USub: lambda: -operand, ast.UAdd: lambda: operand,
                ast.Invert: lambda: ~operand}[type(node.op)]()
    if isinstance(node, ast.BinOp):
        left, right = folded_condition(node.left), folded_condition(node.right)
        operations = {ast.Add: left + right, ast.Sub: left - right, ast.Mult: left * right,
                      ast.BitAnd: left & right, ast.BitOr: left | right, ast.BitXor: left ^ right}
        if type(node.op) in operations:
            return operations[type(node.op)]
        if isinstance(node.op, (ast.Div, ast.FloorDiv)):
            return int(left / right)
        if isinstance(node.op, ast.Mod):
            return left % right
    if isinstance(node, ast.Compare) and len(node.ops) == 1:
        left, right = folded_condition(node.left), folded_condition(node.comparators[0])
        return int({ast.Eq: left == right, ast.NotEq: left != right, ast.Lt: left < right, ast.LtE: left <= right,
                    ast.Gt: left > right, ast.GtE: left >= right}[type(node.ops[0])])
    raise ValueError("not a condition")


# Follows #if, #ifdef, #ifndef, #elif, #else and #endif: conditions is the stack of open blocks,
# each with whether its current branch is compiled and whether any branch has been.
def conditional_directive(word, rest, conditions, defines, line):
    compiling = not conditions or conditions[-1]["on"]
    if word in ("if", "ifdef", "ifndef"):
        name = rest.split("//")[0].strip()
        if word == "if":
            chosen = compiling and condition_value(rest, defines, line)
        else:
            chosen = compiling and ((name in defines) == (word == "ifdef"))
        conditions.append({"parent": compiling, "on": chosen, "taken": chosen or not compiling, "line": line})
        return
    if not conditions:
        lex_fail(line, "#%s without #if" % word)
    block = conditions[-1]
    if word == "elif":
        block["on"] = block["parent"] and not block["taken"] and condition_value(rest, defines, line)
        block["taken"] = block["taken"] or block["on"]
    elif word == "else":
        block["on"] = block["parent"] and not block["taken"]
        block["taken"] = True
    else:
        conditions.pop()


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
    conditions = []
    defines = {}
    position = 0
    line = 1
    while position < len(text):
        char = text[position]
        if char == "\n":
            line += 1
            position += 1
            continue
        directive = DIRECTIVE.match(text, position) if char == "#" else None
        if directive and directive.group(1) in CONDITIONALS:
            conditional_directive(directive.group(1), directive.group(2), conditions, defines, line)
            position = line_end(text, position)
            continue
        if conditions and not conditions[-1]["on"]:
            position = line_end(text, position)
            continue
        if directive and directive.group(1) == "undef":
            defines.pop(directive.group(2).split("//")[0].strip(), None)
            position = line_end(text, position)
            continue
        if char.isspace():
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
        elif text.startswith("#define", position):
            end = line_end(text, position)
            words = text[position + len("#define"):end].split(None, 1)
            if words and "(" in words[0]:
                lex_fail(line, "macros with parameters are not supported; write a function instead")
            if words:
                defines[words[0]] = words[1].split("//")[0].strip() if len(words) > 1 else "1"
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
            word, suffix = match.group(1), match.group(4).lower()
            hexadecimal = word.lower().startswith("0x")
            floating = not hexadecimal and ("." in word or "e" in word.lower() or "f" in suffix)
            if floating and "." not in word and "e" not in word.lower():
                word += ".0"
            tokens.append({"kind": "float" if floating else "int", "text": word, "line": line})
            position = match.end()
        elif char == "'":
            value, position = read_char(text, position, line)
            tokens.append({"kind": "int", "text": str(value), "line": line})
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
        elif text[position:position + 3] in LONG_OPERATORS:
            tokens.append({"kind": "op", "text": text[position:position + 3], "line": line})
            position += 3
        elif text[position:position + 2] in OPERATORS:
            tokens.append({"kind": "op", "text": text[position:position + 2], "line": line})
            position += 2
        elif char in SINGLES:
            tokens.append({"kind": "op", "text": char, "line": line})
            position += 1
        else:
            lex_fail(line, "unexpected character %r" % char)
    if conditions:
        lex_fail(conditions[-1]["line"], "#if without #endif")
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
        node = {"kind": "index", "name": name, "index": index}
    else:
        node = {"kind": "var", "name": name}
    if at(state, "++") or at(state, "--"):
        return postfix(node, take(state)["text"])
    return node


def stepped(target, op):
    return {"kind": "assign", "target": target,
            "value": {"kind": "binary", "op": op, "left": target, "right": {"kind": "int", "value": 1}}}


# x++ and x--: the variable steps, and the expression's value is the old one, worked back from the
# new so no temporary is needed. As a statement only the step remains.
def postfix(target, text):
    op = "+" if text == "++" else "-"
    undo = "-" if op == "+" else "+"
    return {"kind": "binary", "op": undo, "left": stepped(target, op), "right": {"kind": "int", "value": 1}, "postfix": True}


def cast_ahead(state):
    return at(state, "(") and peek(state, 1)["kind"] == "name" and peek(state, 1)["text"] in CAST_TYPES and at(state, ")", 2)


def unary(state):
    if at(state, "-") or at(state, "!"):
        op = take(state)["text"]
        return {"kind": "unary", "op": op, "operand": unary(state)}
    if at(state, "+"):
        take(state)
        return unary(state)
    if at(state, "~"):
        take(state)
        operand = {"kind": "unary", "op": "-", "operand": unary(state)}
        return {"kind": "binary", "op": "-", "left": operand, "right": {"kind": "int", "value": 1}}
    if at(state, "++") or at(state, "--"):
        op = "+" if take(state)["text"] == "++" else "-"
        target = unary(state)
        if target["kind"] not in ("var", "index", "deref"):
            fail(state, "++ and -- need a variable")
        return stepped(target, op)
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


def ternary(state):
    cond = binary(state, 2)
    if not at(state, "?"):
        return cond
    take(state)
    then = assignment(state)
    expect(state, ":")
    return {"kind": "ternary", "cond": cond, "then": then, "else": ternary(state)}


def assignment(state):
    left = ternary(state)
    token = peek(state)
    if token["kind"] != "op" or (token["text"] != "=" and token["text"] not in COMPOUND):
        return left
    if left["kind"] not in ("var", "index", "deref"):
        fail(state, "cannot assign to this expression")
    op = take(state)["text"]
    value = assignment(state)
    if op in COMPOUND:
        value = {"kind": "binary", "op": COMPOUND[op], "left": left, "right": value}
    return {"kind": "assign", "target": left, "value": value}


def expression_statement(node):
    if node.get("postfix"):
        node = node["left"]
    return {"kind": "expr", "expr": node}


def braced(state):
    expect(state, "{")
    statements = []
    while not at(state, "}"):
        statements += statement(state)
    expect(state, "}")
    return statements


# A loop or branch body: a braced block or one statement.
def body(state):
    return braced(state) if at(state, "{") else statement(state)


def conditional(state):
    line = expect(state, "if")["line"]
    expect(state, "(")
    cond = assignment(state)
    expect(state, ")")
    then = body(state)
    otherwise = None
    if at(state, "else"):
        take(state)
        otherwise = [conditional(state)] if at(state, "if") else body(state)
    return {"kind": "if", "cond": cond, "then": then, "else": otherwise, "line": line}


# Puts extra statements in front of every continue of a loop body (not of loops inside it), so a
# for loop's step and a do loop's test run when an iteration is cut short.
def before_continue(nodes, extra):
    out = []
    for node in nodes:
        kind = node["kind"]
        if kind == "continue":
            out += extra
        elif kind == "if":
            otherwise = None if node["else"] is None else before_continue(node["else"], extra)
            node = dict(node, then=before_continue(node["then"], extra), **{"else": otherwise})
        elif kind == "switch":
            node = dict(node, cases=[dict(case, body=before_continue(case["body"], extra)) for case in node["cases"]])
        out.append(node)
    return out


def expression_list(state, closing):
    found = []
    while not at(state, closing):
        found.append(expression_statement(assignment(state)))
        if not at(state, closing):
            expect(state, ",")
    expect(state, closing)
    return found


# for (init; cond; step) body becomes init; while (cond) { body step }, with the step also run
# before each continue.
def for_loop(state):
    line = expect(state, "for")["line"]
    expect(state, "(")
    if declaration_ahead(state):
        init = declaration(state)
    else:
        init = expression_list(state, ";")
    cond = {"kind": "int", "value": 1} if at(state, ";") else assignment(state)
    expect(state, ";")
    step = expression_list(state, ")")
    loop_body = before_continue(body(state), step) + step
    return init + [{"kind": "while", "cond": cond, "body": loop_body, "line": line}]


# do body while (cond); becomes while (1) { body if (!cond) break; }, with the test also run before
# each continue.
def do_loop(state):
    line = expect(state, "do")["line"]
    loop_body = body(state)
    expect(state, "while")
    expect(state, "(")
    cond = assignment(state)
    expect(state, ")")
    expect(state, ";")
    leave = {"kind": "if", "cond": {"kind": "unary", "op": "!", "operand": cond}, "then": [{"kind": "break"}],
             "else": None}
    return [{"kind": "while", "cond": {"kind": "int", "value": 1}, "body": before_continue(loop_body, [leave]) + [leave],
             "line": line}]


def switch(state):
    line = expect(state, "switch")["line"]
    expect(state, "(")
    value = assignment(state)
    expect(state, ")")
    expect(state, "{")
    cases = []
    while not at(state, "}"):
        if at(state, "case"):
            take(state)
            cases.append({"value": ternary(state), "default": False, "body": []})
            expect(state, ":")
        elif at(state, "default"):
            take(state)
            expect(state, ":")
            if any(case["default"] for case in cases):
                fail(state, "a switch has one default")
            cases.append({"value": None, "default": True, "body": []})
        elif not cases:
            fail(state, "expected case or default")
        else:
            cases[-1]["body"] += statement(state)
    expect(state, "}")
    return {"kind": "switch", "value": value, "cases": cases, "line": line}


def statement(state):
    line = peek(state)["line"]
    return [node if "line" in node else dict(node, line=line) for node in statement_nodes(state)]


def statement_nodes(state):
    token = peek(state)
    word = token["text"] if token["kind"] == "name" else None
    if word == "AT":
        take(state)
        expect(state, "(")
        base = int_value(expect_kind(state, "int")["text"])
        expect(state, ")")
        return [dict(node, at=base) for node in statement(state)]
    if word == "DIRECT":
        take(state)
        return [dict(node, at="direct") for node in statement(state)]
    if at(state, "{"):
        return braced(state)
    if at(state, ";"):
        take(state)
        return []
    if declaration_ahead(state):
        return declaration(state)
    if word == "if":
        return [conditional(state)]
    if word == "while":
        take(state)
        expect(state, "(")
        cond = assignment(state)
        expect(state, ")")
        return [{"kind": "while", "cond": cond, "body": body(state)}]
    if word == "for":
        return for_loop(state)
    if word == "do":
        return do_loop(state)
    if word == "switch":
        return [switch(state)]
    if word == "return":
        take(state)
        if at(state, ";"):
            take(state)
            return [{"kind": "return", "value": None}]
        value = assignment(state)
        expect(state, ";")
        return [{"kind": "return", "value": value}]
    if word in SIMPLE:
        take(state)
        expect(state, ";")
        return [{"kind": word}]
    if word == "print":
        take(state)
        args = arguments(state)
        expect(state, ";")
        return [{"kind": "print", "args": args}]
    node = assignment(state)
    expect(state, ";")
    return [expression_statement(node)]


def type_name(state, allowed):
    kind = expect_kind(state, "name")["text"]
    if kind not in allowed:
        state["pos"] -= 1
        fail(state, "unknown type " + kind)
    return kind


def array_size(state):
    token = take(state)
    if token["kind"] == "int":
        return int_value(token["text"])
    if token["kind"] == "name" and isinstance(state.get("constants", {}).get(token["text"]), int):
        return state["constants"][token["text"]]
    state["pos"] -= 1
    fail(state, "an array size is a number or a #define'd number")


def initial_values(state):
    expect(state, "{")
    values = []
    while not at(state, "}"):
        values.append(assignment(state))
        if not at(state, "}"):
            expect(state, ",")
    expect(state, "}")
    return values


def declarators(state, kind):
    found = []
    while True:
        line = peek(state)["line"]
        pointer = False
        if at(state, "*"):
            take(state)
            pointer = True
        name = expect_kind(state, "name")["text"]
        size = None
        if at(state, "["):
            take(state)
            size = None if at(state, "]") else array_size(state)
            expect(state, "]")
            size = -1 if size is None else size
        init = None
        if at(state, "="):
            take(state)
            init = initial_values(state) if at(state, "{") else assignment(state)
        if size == -1:
            if not isinstance(init, list):
                fail(state, "%s[] needs a size or a list of values" % name)
            size = len(init)
        if isinstance(init, list) and (size is None or len(init) > size):
            fail(state, "too many values for " + name)
        declared = {"name": name, "type": kind, "size": size, "init": init, "line": line}
        if pointer:
            declared["pointer"] = True
        found.append(declared)
        if not at(state, ","):
            break
        take(state)
    expect(state, ";")
    return found


def declaration_ahead(state):
    token = peek(state)
    if token["kind"] != "name" or token["text"] not in TYPES:
        return False
    following = peek(state, 1)
    return following["kind"] == "name" or (following["kind"] == "op" and following["text"] == "*")


# A local declaration anywhere in a function. Every local gets a slot in the function's frame, so a
# name declared again in another block reuses the slot when the type matches. Initializers become
# assignments where the declaration stands.
def declaration(state):
    scope = state.get("scope")
    if scope is None:
        fail(state, "declarations belong inside functions")
    kind = take(state)["text"]
    statements = []
    for declared in declarators(state, kind):
        entry = {"name": declared["name"], "type": kind, "size": declared["size"]}
        if declared.get("pointer"):
            entry["pointer"] = True
        if any(param["name"] == entry["name"] for param in scope["params"]):
            fail(state, "%s is already a parameter" % entry["name"])
        existing = [local for local in scope["locals"] if local["name"] == entry["name"]]
        if existing and existing[0] != entry:
            fail(state, "%s is declared again with another type" % entry["name"])
        if not existing:
            scope["locals"].append(entry)
        target = {"kind": "var", "name": entry["name"]}
        if isinstance(declared["init"], list):
            for position, value in enumerate(declared["init"]):
                cell = {"kind": "index", "name": entry["name"], "index": {"kind": "int", "value": position}}
                statements.append({"kind": "expr", "expr": {"kind": "assign", "target": cell, "value": value},
                                   "line": declared["line"]})
        elif declared["init"] is not None:
            statements.append({"kind": "expr", "expr": {"kind": "assign", "target": target, "value": declared["init"]},
                               "line": declared["line"]})
    return statements


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
            pointer = at(state, "*")
            if pointer:
                take(state)
            name = expect_kind(state, "name")["text"]
            array = at(state, "[")
            if array:
                take(state)
                expect(state, "]")
            params.append({"name": name, "type": kind, "array": array or pointer})
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
    state["scope"] = {"params": params, "locals": []}
    body_nodes = braced(state)
    locals_ = state["scope"]["locals"]
    state["scope"] = None
    function = {"kind": "function", "name": name, "returns": returns, "params": params, "locals": locals_,
                "body": body_nodes, "unused_strings": pending.get("strings", []), "padding": pending.get("padding")}
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
        if value["kind"] == "int":
            state.setdefault("constants", {})[words[0]] = value["value"]


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
                        fail(state, "global arrays are not supported: the script VM cannot index a global")
                    if declared.get("pointer"):
                        fail(state, "global pointers are not supported: the script VM cannot index through a global")
                    if declared["init"] is not None:
                        fail(state, "globals start at 0 when the script loads; set %s in a program instead"
                             % declared["name"])
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
