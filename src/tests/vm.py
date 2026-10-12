import re

from c2s import asm, compile, natives, parse, sdk

# A small interpreter for the assembly the compiler writes, so tests can check what compiled code
# does, not only that it compiles. It follows the game's VM (runscript.cpp) for what tests use:
# values keep their type (int, float, string, pointer), locals, arrays and pointers, arithmetic
# with C's truncating division, comparisons, jumps and calls. Natives are not run: a native call
# only takes its arguments off the stack.

LABEL = re.compile(r"^(L_\w+):$")


def assembled(text):
    return compile.to_asm(sdk.resolve(parse.parse(text), natives.load()), natives.load())


def functions(text):
    found = {}
    current = None
    for line in assembled(text).split("\n"):
        line = line.strip()
        if line.startswith(".function "):
            words = line.split()
            current = {"locals": int(words[words.index("local") + 1]), "args": int(words[words.index("arg") + 1]),
                       "code": [], "labels": {}, "strings": []}
            found[words[1]] = current
        elif current is None or not line or line.startswith((".padding", ".natives", ".header", ".program", ".code")):
            continue
        elif line.startswith(".string "):
            current["strings"].append(asm.tokenize(line)[1]["string"])
        elif LABEL.match(line):
            current["labels"][LABEL.match(line).group(1)] = len(current["code"])
        else:
            current["code"].append(line.split())
    return found


def truncated(left, right):
    quotient = abs(left) // abs(right)
    return quotient if (left < 0) == (right < 0) else -quotient


def number(value):
    return value[1]


def arithmetic(op, left, right):
    if op == "div" and left[0] == "int" and right[0] == "int":
        return ("int", truncated(left[1], right[1]))
    if op == "mod":
        return ("int", left[1] - right[1] * truncated(left[1], right[1]))
    if op in ("and", "or"):
        return ("int", left[1] & right[1] if op == "and" else left[1] | right[1])
    kind = "int" if left[0] == "int" and right[0] == "int" else "float"
    result = {"add": left[1] + right[1], "sub": left[1] - right[1], "mul": left[1] * right[1],
              "div": left[1] / right[1] if right[1] else 0}[op]
    return (kind, result)


COMPARES = {"eq": lambda a, b: a == b, "ne": lambda a, b: a != b, "lt": lambda a, b: a < b,
            "le": lambda a, b: a <= b, "gt": lambda a, b: a > b, "ge": lambda a, b: a >= b}


def cell(frame, globals_, mode, slot, stack):
    if mode.startswith("global"):
        return globals_, slot
    if mode.startswith("local_idx"):
        return frame, slot + stack.pop()[1]
    if mode.startswith("ptr_idx"):
        index = stack.pop()[1]
        container, base = frame[slot][1]
        return container, base + index
    return frame, slot


def run_function(table, name, args, globals_):
    function = table[name]
    frame = list(args) + [("int", 0)] * (function["locals"] - len(args))
    stack = []
    pc = 0
    while True:
        words = function["code"][pc]
        pc += 1
        op = words[0]
        if op == "push":
            if words[1] == "string":
                stack.append(("string", function["strings"][int(words[2])]))
            elif re.match(r"^-?\d+$", words[1]):
                stack.append(("int", int(words[1])))
            else:
                stack.append(("float", float(words[1])))
        elif op in ("load", "addr"):
            container, index = cell(frame, globals_, words[1], int(words[2]), stack)
            if op == "addr":
                stack.append(("pointer", (container, index)))
            else:
                value = container[index]
                if words[1].endswith("_f") and value[0] == "int":
                    value = ("float", float(value[1]))
                stack.append(value)
        elif op == "store":
            value = stack.pop()
            container, index = stack.pop()[1]
            container[index] = value
            stack.append(value)
        elif op == "pop":
            stack.pop()
        elif op in ("add", "sub", "mul", "div", "mod", "and", "or"):
            right, left = stack.pop(), stack.pop()
            stack.append(arithmetic(op, left, right))
        elif op == "neg":
            value = stack.pop()
            stack.append((value[0], -value[1]))
        elif op == "not":
            stack.append(("int", int(not stack.pop()[1])))
        elif op == "itof":
            stack.append(("float", float(stack.pop()[1])))
        elif op == "ftoi":
            stack.append(("int", int(stack.pop()[1])))
        elif op == "cmp":
            right, left = stack.pop(), stack.pop()
            stack.append(("int", int(COMPARES[words[1]](left[1], right[1]))))
        elif op == "jmp":
            pc = function["labels"][words[1]]
        elif op in ("jf", "jt", "jf_push", "jt_push"):
            truth = bool(stack.pop()[1])
            if truth == op.startswith("jt"):
                if op.endswith("_push"):
                    stack.append(("int", int(truth)))
                pc = function["labels"][words[1]]
        elif op == "call":
            count = table[words[1]]["args"]
            call_args = stack[len(stack) - count:]
            del stack[len(stack) - count:]
            stack.append(run_function(table, words[1], call_args, globals_))
        elif op == "ret":
            return stack.pop()
        elif op in ("ext", "print"):
            del stack[len(stack) - int(words[1]):]
        elif op in ("nop", "wait", "skip_end"):
            continue
        elif op == "end":
            return ("int", 0)
        else:
            raise ValueError("the test VM does not run " + op)


# Runs a function of a script and returns its result's value.
def run(text, name="f", args=()):
    table = functions(text)
    values = [("int", value) if isinstance(value, int) else ("float", value) for value in args]
    return run_function(table, name, values, [("int", 0)] * 64)[1]
