import ast
import functools
import re
from pathlib import Path

from c2s import definitions, sdk, source

# Reads a decompiled source tree as text, for the commands that answer questions about it (flags,
# xref, fork, changes): the calls each file makes with their arguments, its PROGRAM lines and
# includes, and the constants its headers define. It does not compile anything, so it also reads
# trees that would not compile.

CONSTANT = re.compile(r"^\s*(?:#define\s+([A-Za-z_]\w*)\s+|([A-Za-z_]\w*)\s*=\s*)(-?(?:0x[0-9A-Fa-f]+|\d+))\s*,?\s*(?://.*)?$",
                      re.M)
DEFINITION = re.compile(r"^(?:void|int|float|string)\s+\**(\w+)\s*\(", re.M)
PROGRAM = re.compile(r"^PROGRAM\(\s*([^,]+?)\s*,\s*(\w+)\s*\);", re.M)
INCLUDE = re.compile(r'^#include\s+"([^"]+)"', re.M)


# Every integer constant the headers define: enum members (including program names such as
# story_50) and #defines.
def constants(texts):
    found = {}
    for text in texts.values():
        for match in CONSTANT.finditer(text):
            found[match.group(1) or match.group(2)] = int(match.group(3), 0)
    return found


# The values of one enum the headers declare, such as the program numbers of enum program.
def enum_values(texts, enum_name):
    values = set()
    for text in texts.values():
        for body in re.findall(r"enum\s+%s\s*\{(.*?)\}" % re.escape(enum_name), text, re.S):
            values |= set(constants({"": body}).values())
    return values


# The SDK, each definition set's include folder and the tree's own include folder (disc.h).
def tree_headers(tree, defs, sdk_dir=sdk.SDK_DIR):
    texts = definitions.header_texts(defs, sdk_dir)
    include = Path(tree) / "include"
    if include.is_dir():
        texts.update(sdk.load_texts(include))
    return texts


def without_comments(text):
    lines = []
    for line in text.split("\n"):
        quoted = False
        for position, char in enumerate(line):
            if char == '"' and (position == 0 or line[position - 1] != "\\"):
                quoted = not quoted
            elif not quoted and line.startswith("//", position):
                line = line[:position]
                break
        lines.append(line)
    return "\n".join(lines)


# Whether the end of this text (the start of a line up to a position) is inside a string literal.
def in_string(text):
    quoted = False
    for position, char in enumerate(text):
        if char == '"' and (position == 0 or text[position - 1] != "\\"):
            quoted = not quoted
    return quoted


def split_args(text):
    args, depth, quoted, start = [], 0, False, 0
    for position, char in enumerate(text):
        if char == '"' and (position == 0 or text[position - 1] != "\\"):
            quoted = not quoted
        elif quoted:
            continue
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
        elif char == "," and depth == 0:
            args.append(text[start:position].strip())
            start = position + 1
    last = text[start:].strip()
    return args + [last] if last or args else args


def closing_paren(text, start):
    depth, quoted = 0, False
    for position in range(start, len(text)):
        char = text[position]
        if char == '"' and text[position - 1] != "\\":
            quoted = not quoted
        elif quoted:
            continue
        elif char == "(":
            depth += 1
        elif char == ")":
            depth -= 1
            if depth == 0:
                return position
    return None


# The value of a constant expression as the compiler would fold it: integer literals, header
# constants, + - * / % & | << >> and parentheses, or a string literal. None when it depends on a
# variable or anything else.
def value(expr, consts):
    try:
        node = ast.parse(expr.strip(), mode="eval").body
    except SyntaxError:
        return None
    return folded(node, consts)


def folded(node, consts):
    if isinstance(node, ast.Constant) and isinstance(node.value, (int, str)) and not isinstance(node.value, bool):
        return node.value
    if isinstance(node, ast.Name):
        return consts.get(node.id)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        inner = folded(node.operand, consts)
        return -inner if isinstance(inner, int) else None
    if isinstance(node, ast.BinOp):
        left, right = folded(node.left, consts), folded(node.right, consts)
        if not isinstance(left, int) or not isinstance(right, int):
            return None
        operations = {ast.Add: lambda a, b: a + b, ast.Sub: lambda a, b: a - b, ast.Mult: lambda a, b: a * b,
                      ast.Div: lambda a, b: int(a / b) if b else None, ast.Mod: lambda a, b: a % b if b else None,
                      ast.BitAnd: lambda a, b: a & b, ast.BitOr: lambda a, b: a | b,
                      ast.LShift: lambda a, b: a << b, ast.RShift: lambda a, b: a >> b}
        operation = operations.get(type(node.op))
        return operation(left, right) if operation else None
    return None


# A file's text without comments, read once per run however many questions ask about it.
@functools.lru_cache(maxsize=None)
def code_text(path, modified):
    return without_comments(Path(path).read_text(encoding="utf-8"))


def code(path):
    return code_text(str(path), Path(path).stat().st_mtime_ns)


# Language variants of one script (s51_1.c to s51_5.c) share a key, so reports can list them once.
def variant_key(path):
    return re.sub(r"_[0-9](\.[^./]+)$", r"_N\1", path)


def source_files(tree):
    return sorted(Path(tree).rglob("*" + source.EXTENSION))


def relative(tree, path):
    return Path(path).relative_to(tree).as_posix()


def is_library(path):
    return path.startswith("lib/")


# One record per call to any of names: the file, line, enclosing function and argument texts.
def calls(tree, names):
    pattern = re.compile(r"\b(%s)\s*\(" % "|".join(re.escape(name) for name in sorted(names)))
    found = []
    for path in source_files(tree):
        text = code(path)
        if not pattern.search(text):
            continue
        definitions_at = [(match.start(), match.group(1)) for match in DEFINITION.finditer(text)]
        for match in pattern.finditer(text):
            end = closing_paren(text, match.end() - 1)
            if end is None:
                continue
            line_start = text.rfind("\n", 0, match.start()) + 1
            if DEFINITION.match(text[line_start:match.end()]) or in_string(text[line_start:match.start()]):
                continue
            function = None
            for position, name in definitions_at:
                if position > match.start():
                    break
                function = name
            line_end = text.find("\n", end)
            meant = OLD_NUMBERING.search(text[end:line_end if line_end >= 0 else len(text)])
            found.append({"path": relative(tree, path), "line": text.count("\n", 0, match.start()) + 1,
                          "function": function, "name": match.group(1),
                          "args": split_args(text[match.end():end]), "text": text[match.start():end + 1],
                          "meant": meant.group(1) if meant else None})
    return found


# The decompiler's comment on a call written for an older native table: the engine runs the native
# the call names, but the script meant another.
OLD_NUMBERING = re.compile(r"//.*\bold numbering, meant as (\w+)")


# PROGRAM lines and includes of every file, keyed by tree-relative path.
def outlines(tree):
    found = {}
    for path in source_files(tree):
        text = code(path)
        found[relative(tree, path)] = {"programs": [{"number": number.strip(), "function": function}
                                                    for number, function in PROGRAM.findall(text)],
                                       "includes": INCLUDE.findall(text)}
    return found


# For each library, the scripts that include it, directly or through other libraries.
def includers(outline):
    direct = {}
    for path, entry in outline.items():
        for included in entry["includes"]:
            direct.setdefault(included, set()).add(path)
    result = {}
    for library in direct:
        seen, pending = set(), [library]
        while pending:
            for user in direct.get(pending.pop(), ()):
                if user not in seen:
                    seen.add(user)
                    pending.append(user)
        result[library] = sorted(user for user in seen if not is_library(user))
    return result
