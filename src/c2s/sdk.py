import re
from pathlib import Path

from c2s import names, parse

SDK_DIR = Path(__file__).resolve().parents[2] / "sdk"
PRELUDE = "c2s.h"
EXT_NAME = re.compile(r"^_EXT_\d+$")
EMPTY = {"natives": [], "programs": [], "inlines": [], "constants": [], "kinds": {}}
DEFAULT_LOCAL = re.compile(r"^v\d+$")
KEYWORDS = {"if", "else", "while", "return", "break", "continue", "wait", "end", "skip_end", "print", "sin", "cos",
            "int", "float", "void", "string", "any", "extern", "static", "inline", "enum", "program",
            "NATIVE", "OPTIONAL", "PROGRAM", "MAIN", "AT", "DIRECT"}


def load_texts(directory=SDK_DIR):
    base = Path(directory)
    return {path.relative_to(base).as_posix(): path.read_text(encoding="utf-8") for path in sorted(base.rglob("*.h"))}


def header_declarations(path, headers, declarations, seen):
    if path == PRELUDE or path in seen:
        return None
    if path not in headers:
        raise ValueError("cannot find SDK header <%s>" % path)
    seen.add(path)
    header = parse.parse(headers[path])
    table = header["natives"]
    for key in declarations:
        if key == "kinds":
            declarations[key] = dict(declarations[key], **header["declarations"].get(key, {}))
        else:
            declarations[key] = declarations[key] + header["declarations"].get(key, [])
    for item in header["functions"]:
        if item["kind"] == "include" and item["system"]:
            nested = header_declarations(item["path"], headers, declarations, seen)
            table = table or nested
    return table


def header_name(table):
    return table + ".h"


def context(declarations, table, tables):
    rows = tables[table]["natives"] if table in tables else []
    original_by_number = {row["no"]: row["name"] for row in rows}
    natives = {}
    by_original = {}
    for declared in declarations["natives"]:
        canonical = original_by_number.get(declared["no"], "_EXT_%d" % declared["no"])
        natives[declared["name"]] = dict(declared, canonical=canonical)
        by_original[canonical] = natives[declared["name"]]
    programs = {declared["name"]: declared["no"] for declared in declarations["programs"]}
    constants = {declared["name"]: declared["value"] for declared in declarations.get("constants", [])}
    enums = {}
    float_names = {}
    for declared in declarations["programs"]:
        constants.setdefault(declared["name"], {"kind": "int", "value": declared["no"]})
        enums.setdefault("program", {}).setdefault(declared["no"], declared["name"])
    for declared in declarations.get("constants", []):
        value = declared["value"]
        if declared["enum"] is not None:
            enums.setdefault(declared["enum"], {}).setdefault(value["value"], declared["name"])
        elif value["kind"] == "float":
            float_names.setdefault(value["bits"], declared["name"])
    found = {
        "table": table,
        "natives": natives,
        "by_original": by_original,
        "originals": set(original_by_number.values()),
        "programs": programs,
        "program_names": {number: name for name, number in programs.items()},
        "inlines": {},
        "constants": constants,
        "enums": enums,
        "float_names": float_names,
        "old_groups": {int(base): new for base, new in tables.get(table, {}).get("old_groups", {}).items()},
        "by_number": {declared["no"]: declared["name"] for declared in declarations["natives"]},
    }
    for inline in declarations["inlines"]:
        body = with_constants(inline["body"], constants, {param["name"] for param in inline["params"]})
        found["inlines"][inline["name"]] = dict(inline, body=statements(body, found))
    return found


def header_context(table, headers, tables, extra=()):
    name = header_name(table)
    if name not in headers:
        return None
    declarations = {key: [] for key in EMPTY}
    seen = set()
    header_declarations(name, headers, declarations, seen)
    for include in extra:
        header_declarations(include, headers, declarations, seen)
    return dict(context(declarations, table, tables), includes=[name] + list(extra))


def reserved(found):
    if found is None:
        return set(KEYWORDS)
    return set(found["natives"]) | set(found["inlines"]) | set(found["programs"]) | set(found["constants"]) | KEYWORDS


def with_constants(node, constants, shadowed):
    if isinstance(node, list):
        return [with_constants(item, constants, shadowed) for item in node]
    if not isinstance(node, dict):
        return node
    if node.get("kind") == "var" and node["name"] in constants and node["name"] not in shadowed:
        return constants[node["name"]]
    return {key: with_constants(value, constants, shadowed) for key, value in node.items()}


def variable_names(function, global_names):
    return ({param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
            | global_names)


def is_native(name, found):
    return name in found["natives"] or name in found["originals"] or bool(EXT_NAME.match(name))


def has_result(declared):
    return bool(declared["params"]) and declared["params"][-1].get("result", False)


def canonical_native(name, at, args, found):
    if name in found["natives"]:
        declared = found["natives"][name]
        if at is None:
            base = declared["base"]
        elif at == "direct":
            base = None
        else:
            base = at
        return {"kind": "native", "name": declared["canonical"], "base": base, "args": args}
    return {"kind": "native", "name": name, "base": None if at in (None, "direct") else at, "args": args}


def address_of(target):
    if target["kind"] == "deref":
        return target["operand"]
    return {"kind": "addr", "target": target}


def substituted(node, bindings, outs):
    if isinstance(node, list):
        return [substituted(item, bindings, outs) for item in node]
    if not isinstance(node, dict):
        return node
    if node.get("kind") == "var" and node["name"] in bindings:
        return bindings[node["name"]]
    if (node.get("kind") == "deref" and node["operand"].get("kind") == "var"
            and node["operand"]["name"] in outs):
        return bindings[node["operand"]["name"]]["target"]
    return {key: substituted(value, bindings, outs) for key, value in node.items() if key != "line"}


def expanded_inline(inline, args):
    if len(args) != len(inline["params"]):
        raise ValueError("%s takes %d arguments" % (inline["name"], len(inline["params"])))
    bindings = {param["name"]: arg for param, arg in zip(inline["params"], args)}
    outs = {param["name"] for param in inline["params"] if param.get("out")}
    for name in outs:
        if bindings[name]["kind"] != "addr":
            raise ValueError("%s: pass &variable for pointer parameter %s" % (inline["name"], name))
    return substituted(inline["body"], bindings, outs)


def native_values(node, found, out):
    if isinstance(node, list):
        for item in node:
            native_values(item, found, out)
    elif isinstance(node, dict):
        if node.get("kind") == "call" and is_native(node["name"], found):
            out.append(node["name"])
        for value in node.values():
            native_values(value, found, out)
    return out


def checked(node, found):
    misplaced = native_values(node, found, [])
    if misplaced:
        raise ValueError("native %s has no value; call it as a statement or assign its result to a variable"
                         % misplaced[0])
    return node


# Native calls inside an expression. The VM's natives push no value, so each one is called first,
# into a hidden local (__value_N), and the expression reads that local. The calls are added to
# before, in the order the expression would make them; one in a condition therefore runs before
# the condition is tested, even on the right of && or || or in a branch of ?:.
def hoisted(node, found, temps, before):
    if isinstance(node, list):
        return [hoisted(item, found, temps, before) for item in node]
    if not isinstance(node, dict):
        return node
    if node.get("kind") == "call" and node["name"] in found["inlines"]:
        raise ValueError("helper %s is a statement; call it on a line of its own" % node["name"])
    if node.get("kind") == "call" and is_native(node["name"], found):
        declared = found["natives"].get(node["name"])
        if declared is None or not has_result(declared):
            raise ValueError("native %s returns nothing, so it has no value to use" % node["name"])
        args = hoisted(node["args"], found, temps, before)
        name = "__value_%d" % len(temps)
        temps.append(name)
        target = {"kind": "addr", "target": {"kind": "var", "name": name}}
        before.append(canonical_native(node["name"], None, args + [target], found))
        return {"kind": "var", "name": name}
    return {key: hoisted(value, found, temps, before) for key, value in node.items()}


def resolved_statement(node, found, temps, before):
    kind = node["kind"]
    at = node.get("at")
    plain = {key: value for key, value in node.items() if key != "at"}
    expression = node.get("expr") if kind == "expr" else None

    def value(item):
        return hoisted(item, found, temps, before)

    if expression is not None and expression["kind"] == "call":
        name = expression["name"]
        if name in found["inlines"]:
            if at is not None:
                raise ValueError("AT and DIRECT apply to native calls, not to helper " + name)
            return expanded_inline(found["inlines"][name], value(expression["args"]))
        if is_native(name, found):
            return [canonical_native(name, at, value(expression["args"]), found)]
    if (expression is not None and expression["kind"] == "assign" and expression["value"]["kind"] == "call"
            and is_native(expression["value"]["name"], found)):
        name = expression["value"]["name"]
        declared = found["natives"].get(name)
        if declared is None or not has_result(declared):
            raise ValueError("native %s returns nothing; pass &variable instead of assigning" % name)
        args = value(expression["value"]["args"])
        args.append(address_of(value(expression["target"])))
        return [canonical_native(name, at, args, found)]
    if at is not None:
        raise ValueError("AT and DIRECT apply only to native calls")
    if kind == "native":
        return [canonical_native(node["name"], node["base"], value(node["args"]), found)]
    if kind == "if":
        otherwise = None if node["else"] is None else statements(node["else"], found, temps)
        return [dict(plain, cond=value(node["cond"]), then=statements(node["then"], found, temps),
                     **{"else": otherwise})]
    if kind == "while":
        inner = []
        cond = hoisted(node["cond"], found, temps, inner)
        body = statements(node["body"], found, temps)
        if not inner:
            return [dict(plain, cond=cond, body=body)]
        # The condition's native calls run at the top of every iteration.
        leave = {"kind": "if", "cond": {"kind": "unary", "op": "!", "operand": cond}, "then": [{"kind": "break"}],
                 "else": None}
        return [dict(plain, cond={"kind": "int", "value": 1}, body=inner + [leave] + body)]
    if kind == "switch":
        cases = [dict(case, value=None if case["value"] is None else checked(case["value"], found),
                      body=statements(case["body"], found, temps)) for case in node["cases"]]
        return [dict(plain, value=value(node["value"]), cases=cases)]
    return [value(plain)]


def statement(node, found, temps=None):
    temps = [] if temps is None else temps
    before = []
    result = before + resolved_statement(node, found, temps, before)
    line = node.get("line")
    if line is None:
        return result
    return [item if "line" in item else dict(item, line=line) for item in result]


def statements(nodes, found, temps=None):
    temps = [] if temps is None else temps
    out = []
    for node in nodes:
        try:
            out += statement(node, found, temps)
        except ValueError as error:
            if node.get("line") is None or str(error).startswith("line "):
                raise
            raise ValueError("line %d: %s" % (node["line"], error)) from None
    return out


# Locals whose array size names a constant get its value.
def sized_locals(function, found):
    sized = []
    for declared in function["locals"]:
        size = declared["size"]
        if isinstance(size, str):
            value = found["constants"].get(size)
            if not value or value.get("kind") != "int":
                raise ValueError("%s: array size %s is not a known integer constant" % (function["name"], size))
            declared = dict(declared, size=value["value"])
        sized.append(declared)
    return sized


def resolve(program, tables):
    if "asm" in program:
        return program
    found = context(program.get("declarations", EMPTY), program["natives"], tables)
    global_names = {declared["name"] for declared in program["globals"]}
    functions = []
    for function in program["functions"]:
        if function["kind"] == "function":
            body = with_constants(function["body"], found["constants"], variable_names(function, global_names))
            temps = []
            function = dict(function, body=statements(body, found, temps), locals=sized_locals(function, found))
            if temps:
                function["locals"] = function["locals"] + [{"name": name, "type": "int", "size": None} for name in temps]
        functions.append(function)
    programs = []
    for entry in program["programs"]:
        number = entry["no"]
        if isinstance(number, str):
            if number not in found["programs"]:
                raise ValueError("unknown program name " + number)
            number = found["programs"][number]
        programs.append(dict(entry, no=number))
    return dict(program, functions=functions, programs=programs)


def matches(pattern, node, params, bindings):
    if isinstance(pattern, list):
        return (isinstance(node, list) and len(pattern) == len(node)
                and all(matches(item, other, params, bindings) for item, other in zip(pattern, node)))
    if not isinstance(pattern, dict):
        return pattern == node
    if not isinstance(node, dict):
        return False
    if pattern.get("kind") == "var" and pattern["name"] in params:
        if params[pattern["name"]] == "out" and node.get("kind") != "addr":
            return False
        if params[pattern["name"]] == "in" and node.get("kind") == "addr":
            return False
        return bound(pattern["name"], node, bindings)
    if (pattern.get("kind") == "deref" and pattern["operand"].get("kind") == "var"
            and params.get(pattern["operand"]["name"]) == "out"):
        if node.get("kind") not in ("var", "index"):
            return False
        return bound(pattern["operand"]["name"], {"kind": "addr", "target": node}, bindings)
    keys = set(pattern) - {"line"}
    if keys != set(node) - {"line"}:
        return False
    return all(matches(pattern[key], node[key], params, bindings) for key in keys)


def bound(name, node, bindings):
    if name in bindings:
        return bindings[name] == node
    bindings[name] = node
    return True


def ordered_inlines(found):
    ranked = sorted((-len(inline["body"]), name) for name, inline in found["inlines"].items())
    return [found["inlines"][name] for _, name in ranked]


def helper_call(inline, nodes, position):
    size = len(inline["body"])
    params = {param["name"]: "out" if param.get("out") else "any" if param["type"] == "any" else "in"
              for param in inline["params"]}
    bindings = {}
    if not matches(inline["body"], nodes[position:position + size], params, bindings) or set(bindings) != set(params):
        return None
    args = [bindings[param["name"]] for param in inline["params"]]
    return {"kind": "expr", "expr": {"kind": "call", "name": inline["name"], "args": args}, "params": inline["params"]}


def folded(nodes, found, inlines, pointers):
    best = [(0, 0)] * (len(nodes) + 1)
    choice = [None] * len(nodes)
    for position in range(len(nodes) - 1, -1, -1):
        statements_after, natives_after = best[position + 1]
        best[position] = (statements_after + 1, natives_after + (1 if nodes[position]["kind"] == "native" else 0))
        for inline in inlines:
            size = len(inline["body"])
            if position + size > len(nodes):
                continue
            cost = (best[position + size][0] + 1, best[position + size][1])
            if cost >= best[position]:
                continue
            call = helper_call(inline, nodes, position)
            if call is not None:
                best[position] = cost
                choice[position] = (call, size)
    out = []
    position = 0
    while position < len(nodes):
        if choice[position] is None:
            out.append(nested(nodes[position], found, inlines, pointers))
            position += 1
        else:
            call = choice[position][0]
            args = named_args(call["expr"]["args"], call.pop("params"), found)
            call = dict(call, expr=dict(call["expr"], args=args))
            comments = [node["comment"] for node in nodes[position:position + choice[position][1]] if node.get("comment")]
            if comments:
                call["comment"] = "; ".join(comments)
            out.append(call)
            position += choice[position][1]
    return out


def nested(node, found, inlines, pointers):
    if node["kind"] == "if":
        otherwise = None if node["else"] is None else folded(node["else"], found, inlines, pointers)
        return dict(node, then=folded(node["then"], found, inlines, pointers), **{"else": otherwise})
    if node["kind"] == "while":
        return dict(node, body=folded(node["body"], found, inlines, pointers))
    if node["kind"] == "native":
        return readable_native(node, found, pointers)
    return node


def result_target(arg, pointers):
    if arg["kind"] == "addr" and arg["target"]["kind"] in ("var", "index"):
        return arg["target"]
    if arg["kind"] == "var" and arg["name"] in pointers:
        return {"kind": "deref", "operand": arg}
    return None


def enum_values(name, found):
    values = {}
    for enum, members in sorted(found["enums"].items()):
        if enum == name or enum.startswith(name + "_"):
            for value, constant in members.items():
                values.setdefault(value, constant)
    return values


def named_literal(node, enum, found):
    names = enum_values(enum, found)
    if node["kind"] == "int" and node["value"] in names:
        return {"kind": "var", "name": names[node["value"]]}
    if node["kind"] == "binary" and node["op"] in ("|", "+", "-", "*", "/"):
        return dict(node, left=named_literal(node["left"], enum, found), right=named_literal(node["right"], enum, found))
    return node


def named_args(args, params, found):
    out = []
    for position, arg in enumerate(args):
        enum = (params[position].get("enum") or params[position].get("kind")) if position < len(params) else None
        out.append(named_literal(arg, enum, found) if enum else arg)
    return out


def named_floats(node, found):
    if isinstance(node, list):
        return [named_floats(item, found) for item in node]
    if not isinstance(node, dict):
        return node
    if node.get("kind") == "float" and node["bits"] in found["float_names"]:
        return {"kind": "var", "name": found["float_names"][node["bits"]]}
    return {key: named_floats(value, found) for key, value in node.items()}


def readable_native(node, found, pointers):
    declared = found["by_original"].get(node["name"])
    if declared is None:
        return dict(node, result=None)
    node = dict(node, args=named_args(node["args"], declared["params"], found))
    base = node["base"]
    if base == declared["base"]:
        base = None
    elif base is None:
        base = "direct"
    args = node["args"]
    result = None
    if has_result(declared) and len(args) == len(declared["params"]):
        result = result_target(args[-1], pointers)
        if result is not None:
            args = args[:-1]
    return dict(node, name=declared["name"], base=base, args=args, result=result)


def out_uses(nodes, found, out):
    for node in nodes:
        if node["kind"] == "native":
            declared = found["by_original"].get(node["name"])
            for position, arg in enumerate(node["args"]):
                if (declared is not None and position < len(declared["params"]) and declared["params"][position]["out"]
                        and arg["kind"] == "addr" and arg["target"]["kind"] == "var"):
                    out.append((arg["target"]["name"], declared["params"][position]["name"]))
        elif node["kind"] == "if":
            out_uses(node["then"], found, out)
            out_uses(node["else"] or [], found, out)
        elif node["kind"] == "while":
            out_uses(node["body"], found, out)
    return out


def named_locals(function, found):
    unnamed = {declared["name"] for declared in function["locals"] if DEFAULT_LOCAL.match(declared["name"])}
    if not unnamed:
        return function
    taken = ({param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
             | set(names.variables(function["body"], [])) | KEYWORDS | reserved(found))
    mapping = {}
    for variable, wanted in out_uses(function["body"], found, []):
        if variable not in unnamed or variable in mapping:
            continue
        candidate = wanted
        copy = 1
        while candidate in taken:
            copy += 1
            candidate = "%s_%d" % (wanted, copy)
        mapping[variable] = candidate
        taken.add(candidate)
    if not mapping:
        return function
    return dict(function,
                locals=[dict(declared, name=mapping.get(declared["name"], declared["name"])) for declared in function["locals"]],
                body=names.renamed_variables(function["body"], mapping))


def call_named(node, found, function_kinds):
    if isinstance(node, list):
        return [call_named(item, found, function_kinds) for item in node]
    if not isinstance(node, dict):
        return node
    out = {key: call_named(value, found, function_kinds) for key, value in node.items()}
    if node.get("kind") == "call" and node["name"] in function_kinds:
        kinds = function_kinds[node["name"]]
        out["args"] = [named_literal(arg, kinds[position], found) if position in kinds else arg
                       for position, arg in enumerate(out["args"])]
    return out


def function_kinds_of(params_by_kind):
    found = {}
    for kind, functions in params_by_kind.items():
        for name, positions in functions.items():
            for position in positions:
                found.setdefault(name, {})[position] = kind
    return found


def present_functions(functions, found, params_by_kind=None):
    if found is None:
        return functions
    function_kinds = function_kinds_of(params_by_kind or {})
    if function_kinds:
        functions = [dict(function, body=call_named(function["body"], found, function_kinds))
                     if function["kind"] == "function" else function for function in functions]
    inlines = ordered_inlines(found)
    out = []
    for function in functions:
        if function["kind"] == "function":
            if found["old_groups"]:
                function = dict(function, body=old_numbered(function["body"], found))
            function = named_locals(function, found)
            body = folded(function["body"], found, inlines, set())
            if found["float_names"] and not shadowing(function, found["float_names"].values()):
                body = named_floats(body, found)
            function = dict(function, body=body)
        out.append(function)
    return out


def meant_native(node, found):
    declared = found["by_original"].get(node["name"])
    if declared is None or node["base"] not in found["old_groups"]:
        return None
    return found["by_number"].get(declared["no"] - node["base"] + found["old_groups"][node["base"]])


def old_numbered(nodes, found):
    out = []
    for node in nodes:
        if node["kind"] == "if":
            otherwise = None if node["else"] is None else old_numbered(node["else"], found)
            node = dict(node, then=old_numbered(node["then"], found), **{"else": otherwise})
        elif node["kind"] == "while":
            node = dict(node, body=old_numbered(node["body"], found))
        elif node["kind"] == "native" and meant_native(node, found):
            note = "old numbering, meant as " + meant_native(node, found)
            node = dict(node, comment="; ".join(part for part in (note, node.get("comment")) if part))
        out.append(node)
    return out


def shadowing(function, names_used):
    declared = {param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
    return bool(declared & set(names_used))


def present_inline(inline, found):
    pointers = {param["name"] for param in inline["params"] if param.get("out")}
    return dict(inline, body=folded(inline["body"], found, [], pointers))


def present(program, found, params_by_kind=None):
    if found is None or "asm" in program:
        return program
    programs = [dict(entry, no=found["program_names"].get(entry["no"], entry["no"])) for entry in program["programs"]]
    includes = [{"kind": "include", "path": path, "system": True} for path in found["includes"]]
    return dict(program, natives=None, programs=programs,
                functions=includes + present_functions(program["functions"], found, params_by_kind))
