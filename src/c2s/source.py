import textwrap

from c2s import asm

PRECEDENCE = {
    "||": 2, "&&": 3, "|": 4, "&": 5,
    "==": 6, "!=": 6, "<": 7, "<=": 7, ">": 7, ">=": 7,
    "+": 8, "-": 8, "*": 9, "/": 9, "%": 9,
}
ASSIGN = 1
UNARY = 10
PRIMARY = 11
INDENT = "    "


def float_literal(bits):
    text = asm.float_text(bits)
    if text is None:
        raise ValueError("float constant without a decimal form")
    return text


def wrap(text, precedence, needed):
    return "(" + text + ")" if precedence < needed else text


def arguments(nodes):
    return ", ".join(expression(node)[0] for node in nodes)


def expression(node):
    kind = node["kind"]
    if kind == "int":
        return str(node["value"]), PRIMARY
    if kind == "float":
        return float_literal(node["bits"]), PRIMARY
    if kind == "string":
        return asm.quote(node["text"]), PRIMARY
    if kind == "var":
        return node["name"], PRIMARY
    if kind == "index":
        return "%s[%s]" % (node["name"], expression(node["index"])[0]), PRIMARY
    if kind == "addr":
        if node["target"]["kind"] == "deref":
            return expression(node["target"]["operand"])
        return "&" + expression(node["target"])[0], UNARY
    if kind == "deref":
        operand, precedence = expression(node["operand"])
        return "*" + wrap(operand, precedence, UNARY), UNARY
    if kind == "assign":
        target = expression(node["target"])[0]
        value, precedence = expression(node["value"])
        return "%s = %s" % (target, wrap(value, precedence, ASSIGN)), ASSIGN
    if kind in ("binary", "logical"):
        op = node["op"]
        needed = PRECEDENCE[op]
        left, left_precedence = expression(node["left"])
        right, right_precedence = expression(node["right"])
        text = "%s %s %s" % (wrap(left, left_precedence, needed), op, wrap(right, right_precedence, needed + 1))
        return text, needed
    if kind == "unary":
        operand, precedence = expression(node["operand"])
        if operand.startswith(("-", "!")):
            precedence = 0
        return node["op"] + wrap(operand, precedence, UNARY), UNARY
    if kind == "cast":
        operand, precedence = expression(node["operand"])
        return "(%s)%s" % (node["type"], wrap(operand, precedence, UNARY)), UNARY
    if kind == "builtin":
        return "%s(%s)" % (node["name"], expression(node["operand"])[0]), PRIMARY
    if kind == "call":
        return "%s(%s)" % (node["name"], arguments(node["args"])), PRIMARY
    raise ValueError("unknown expression " + kind)


def statement(node, depth):
    lines = bare_statement(node, depth)
    if node.get("comment"):
        if len(lines) == 1:
            lines = [lines[0] + "  // " + node["comment"]]
        else:
            lines = [INDENT * depth + "// " + node["comment"]] + lines
    return lines


def bare_statement(node, depth):
    pad = INDENT * depth
    kind = node["kind"]
    if kind == "expr":
        return [pad + expression(node["expr"])[0] + ";"]
    if kind == "return":
        if node["value"] is None:
            return [pad + "return;"]
        return [pad + "return " + expression(node["value"])[0] + ";"]
    if kind == "native":
        return [pad + native_text(node)]
    if kind == "print":
        return [pad + "print(%s);" % arguments(node["args"])]
    if kind in ("wait", "end", "skip_end", "break", "continue"):
        return [pad + kind + ";"]
    if kind == "if":
        return conditional(node, depth, pad + "if")
    if kind == "while":
        return [pad + "while (%s) {" % expression(node["cond"])[0]] + block(node["body"], depth + 1) + [pad + "}"]
    raise ValueError("unknown statement " + kind)


def native_text(node):
    base = node["base"]
    prefix = "" if base is None else "DIRECT " if base == "direct" else "AT(%d) " % base
    call = "%s(%s);" % (node["name"], arguments(node["args"]))
    if node.get("result") is not None:
        call = "%s = %s" % (expression(node["result"])[0], call)
    return prefix + call


def conditional(node, depth, opening):
    pad = INDENT * depth
    lines = ["%s (%s) {" % (opening, expression(node["cond"])[0])] + block(node["then"], depth + 1)
    otherwise = node["else"]
    if otherwise is None:
        return lines + [pad + "}"]
    if len(otherwise) == 1 and otherwise[0]["kind"] == "if":
        return lines + conditional(otherwise[0], depth, pad + "} else if")
    return lines + [pad + "} else {"] + block(otherwise, depth + 1) + [pad + "}"]


def block(statements, depth):
    lines = []
    for node in statements:
        lines += statement(node, depth)
    return lines


def parameter(param):
    return "%s %s%s" % (param["type"], param["name"], "[]" if param["array"] else "")


def parameter_list(texts):
    return ", ".join(texts) if texts else "void"


def doc_lines(text):
    lines = []
    for paragraph in text.split("\n"):
        if lines:
            lines.append("//")
        lines += ["// " + row for row in textwrap.wrap(paragraph, 96)]
    return lines


def function_text(function):
    lines = []
    if function["unused_strings"]:
        lines.append("#pragma strings " + " ".join(asm.quote(text) for text in function["unused_strings"]))
    if function.get("doc"):
        lines += doc_lines(function["doc"])
    params = parameter_list([parameter(param) for param in function["params"]])
    lines.append("%s %s(%s)" % (function.get("returns", "int"), function["name"], params))
    lines.append("{")
    for declared in function["locals"]:
        size = "" if declared["size"] is None else "[%d]" % declared["size"]
        lines.append("%s%s %s%s;" % (INDENT, declared["type"], declared["name"], size))
    if function["locals"] and function["body"]:
        lines.append("")
    lines += block(function["body"], 1)
    lines.append("}")
    return lines


def asm_text(function):
    return ["// " + function["reason"], "asm %s arg %d {" % (function["name"], function["arg"])] + function["lines"] + ["}"]


def text(program):
    if "asm" in program:
        return "#pragma assembly\n" + program["asm"]
    leading = 0
    while (leading < len(program["functions"]) and program["functions"][leading]["kind"] == "include"
           and program["functions"][leading]["system"]):
        leading += 1
    lines = doc_lines(program["doc"]) + [""] if program.get("doc") else []
    lines += [include_line(item) for item in program["functions"][:leading]]
    if program["natives"] is not None:
        lines.append("#pragma natives " + program["natives"])
    magic = asm.magic_text(program["magic"])
    if magic != "SB2":
        lines.append("#pragma magic " + magic)
    if program["unk_14"] != 0:
        lines.append("#pragma unk_14 %d" % program["unk_14"])
    if program["header_padding"] != asm.HEADER_PADDING:
        lines.append("#pragma header_padding " + program["header_padding"])
    if program["global_num"] != len(program["globals"]):
        lines.append("#pragma globals %d" % program["global_num"])
    lines.append("")
    for declared in program["globals"]:
        lines.append("%s %s;" % (declared["type"], declared["name"]))
    if program["globals"]:
        lines.append("")
    for entry in program["programs"]:
        lines.append("PROGRAM(%s, %s);" % (entry["no"], entry["function"]))
    lines.append("MAIN(%s);" % program["main"])
    lines += function_lines(program["functions"][leading:])
    padding_of = dict(program["padding_of"])
    for function in program["functions"]:
        if function["kind"] == "function" and function["padding"] is not None:
            padding_of[function["name"]] = function["padding"]
    if padding_of:
        lines.append("")
        for name, padding in padding_of.items():
            lines.append(("#pragma padding_of %s %s" % (name, padding)).rstrip())
    return "\n".join(lines) + "\n"


def function_lines(functions):
    lines = []
    previous = None
    for function in functions:
        if function["kind"] == "include":
            if previous != "include":
                lines.append("")
            lines.append(include_line(function))
        else:
            lines.append("")
            lines += asm_text(function) if function["kind"] == "asm" else function_text(function)
        previous = function["kind"]
    return lines


def include_line(item):
    if item["system"]:
        return "#include <%s>" % item["path"]
    return '#include "%s"' % item["path"]


def library_text(table, functions, header, doc=""):
    first = "#include <%s>" % header if header else "#pragma natives " + table
    lines = doc_lines(doc) + [""] if doc else []
    return "\n".join(lines + [first] + function_lines(functions)) + "\n"


def typed_parameter(param):
    pointer = "*" if param.get("out") else ""
    kind = "enum " + param["enum"] if param.get("enum") else param.get("kind") or param["type"]
    return "%s %s%s" % (kind, pointer, param["name"])


def inline_text(inline):
    params = parameter_list([typed_parameter(param) for param in inline["params"]])
    return ["static inline void %s(%s)" % (inline["name"], params), "{"] + block(inline["body"], 1) + ["}"]


def native_declaration(declared):
    params = [param for param in declared["params"] if not param.get("result")]
    texts = [typed_parameter(param) for param in params if not param.get("optional")]
    optional = [typed_parameter(param) for param in params if param.get("optional")]
    if optional:
        texts.append("OPTIONAL(%s)" % ", ".join(optional + (["..."] if declared["variadic"] else [])))
    elif declared["variadic"]:
        texts.append("...")
    number = str(declared["no"]) if declared["base"] is None else "%d, %d" % (declared["base"], declared["no"] - declared["base"])
    return "extern %s %s(%s) NATIVE(%s);" % (declared["returns"], declared["name"], parameter_list(texts), number)
