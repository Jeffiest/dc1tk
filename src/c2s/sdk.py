import collections
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
        "old_direct": {int(number): new for number, new in tables.get(table, {}).get("old_direct", {}).items()},
        "old_table_groups": {int(base): new for base, new in tables.get(table, {}).get("old_table_groups", {}).items()},
        "old_table": False,
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
    return {key: substituted(value, bindings, outs) for key, value in node.items() if key not in ("line", "start")}


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
    if node.get("note") and result and not result[0].get("note"):
        result[0] = dict(result[0], note=node["note"])
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


BASE_TYPES = ("int", "float", "string")


# A variable declared with a value kind (bgm_file) or an enum (enum se_bank) has the kind's base
# type, int for an enum; the kind stays on it, for the decompiler's types.
def typed(variable, kinds):
    kind = variable["type"]
    if kind in BASE_TYPES:
        return variable
    base = "int" if kind.startswith("enum ") else kinds.get(kind)
    if base not in BASE_TYPES:
        raise ValueError("%s: unknown type %s" % (variable["name"], kind))
    return dict(variable, type=base, kind=kind)


def resolve(program, tables):
    if "asm" in program:
        return program
    found = context(program.get("declarations", EMPTY), program["natives"], tables)
    global_names = {declared["name"] for declared in program["globals"]}
    kinds = program.get("declarations", EMPTY).get("kinds", {})
    functions = []
    for function in program["functions"]:
        if function["kind"] == "function":
            function = dict(function, params=[typed(param, kinds) for param in function["params"]],
                            locals=[typed(declared, kinds) for declared in function["locals"]])
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
    keys = set(pattern) - {"line", "start"}
    if keys != set(node) - {"line", "start"}:
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


# Kinds whose constant, when a native is called with one, names what the native's result is about:
# get_flag(FLAG_GRIFFON_DEFEATED) reads griffon_defeated, get_chara_pos(CHARA_PLAYER, ...) the
# player's x, y and z.
SUBJECT_KINDS = {"flag": "FLAG_", "chara_slot": "CHARA_"}
# Natives that read a vector of something (get_home_pos, get_target_rot), whose x, y and z results
# are named for that something.
VECTOR_GETTER = re.compile(r"^get_([a-z][a-z_]*?)_(?:pos|rot|vec|dir|scale)$")
VECTOR_PARTS = ("x", "y", "z")


def call_subject(node, declared, found):
    for position, arg in enumerate(node["args"]):
        if position >= len(declared["params"]) or arg["kind"] != "int":
            continue
        kind = declared["params"][position].get("kind")
        if kind in SUBJECT_KINDS:
            constant = enum_values(kind, found).get(arg["value"])
            if constant and constant.startswith(SUBJECT_KINDS[kind]):
                return kind, constant[len(SUBJECT_KINDS[kind]):].lower()
    return None, None


def out_uses(nodes, found, out):
    for node in nodes:
        if node["kind"] == "native":
            declared = found["by_original"].get(node["name"])
            if declared is None:
                continue
            kind, subject = call_subject(node, declared, found)
            outs = [(position, arg) for position, arg in enumerate(node["args"])
                    if position < len(declared["params"]) and declared["params"][position]["out"]
                    and arg["kind"] == "addr" and arg["target"]["kind"] == "var"]
            # get_home_pos(&x, &y, &z) reads the home's position: home_x, home_y, home_z
            thing = VECTOR_GETTER.match(declared["name"])
            for position, arg in outs:
                wanted = declared["params"][position]["name"]
                if subject and kind == "flag" and len(outs) == 1:
                    wanted = subject
                elif subject and kind == "chara_slot":
                    wanted = "%s_%s" % (subject, wanted)
                elif thing and thing.group(1) not in ("chara", "actor", "handle", "object") and wanted in VECTOR_PARTS:
                    wanted = "%s_%s" % (thing.group(1), wanted)
                out.append((arg["target"]["name"], wanted, declared["params"][position]["name"]))
        elif node["kind"] == "if":
            out_uses(node["then"], found, out)
            out_uses(node["else"] or [], found, out)
        elif node["kind"] == "while":
            out_uses(node["body"], found, out)
    return out


# Names locals after the natives that fill them: a slot-numbered local takes the name of the result
# it first holds, and a local with only that generic name (flag, flag_2) takes the name of what it
# reads when every read agrees (griffon_defeated for get_flag(FLAG_GRIFFON_DEFEATED)).
def named_locals(function, found):
    uses = out_uses(function["body"], found, [])
    by_variable = collections.defaultdict(set)
    for variable, wanted, generic in uses:
        by_variable[variable].add((wanted, generic))
    generic_names = set()
    for declared in function["locals"]:
        seen = by_variable.get(declared["name"], set())
        if len(seen) == 1:
            wanted, generic = next(iter(seen))
            if wanted != generic and re.fullmatch(r"%s(_\d+)?" % re.escape(generic), declared["name"]):
                generic_names.add(declared["name"])
    unnamed = {declared["name"] for declared in function["locals"] if DEFAULT_LOCAL.match(declared["name"])} | generic_names
    if not unnamed:
        return function
    taken = ({param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
             | set(names.variables(function["body"], [])) | KEYWORDS | reserved(found))
    mapping = {}
    for variable, wanted, _ in uses:
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


# The name an enum type's constants are looked up by: enum se_bank reads as the kind se_bank.
def kind_name(kind):
    return kind[len("enum "):] if kind.startswith("enum ") else kind


def variable_kinds(function):
    return {variable["name"]: kind_name(variable["kind"]) for variable in function["params"] + function["locals"]
            if variable.get("kind")}


COMPARED = ("==", "!=", "<", "<=", ">", ">=")


# Names the numbers assigned to a variable with a value kind, and compared with it.
def typed_literals(node, kinds, found):
    if not kinds:
        return node
    if isinstance(node, list):
        return [typed_literals(item, kinds, found) for item in node]
    if not isinstance(node, dict):
        return node
    out = {key: typed_literals(value, kinds, found) for key, value in node.items()}
    kind = out.get("kind")
    if kind == "assign" and out["target"]["kind"] == "var" and out["target"]["name"] in kinds:
        out["value"] = named_literal(out["value"], kinds[out["target"]["name"]], found)
    elif kind == "binary" and out["op"] in COMPARED:
        for side, other in (("left", "right"), ("right", "left")):
            if out[side]["kind"] == "var" and out[side]["name"] in kinds:
                out[other] = named_literal(out[other], kinds[out[side]["name"]], found)
    return out


def present_functions(functions, found, params_by_kind=None):
    if found is None:
        return functions
    function_kinds = function_kinds_of(params_by_kind or {})
    # parameters a definition set gives a value kind (types)
    for function in functions:
        if function["kind"] == "function":
            for position, param in enumerate(function["params"]):
                if param.get("kind"):
                    function_kinds.setdefault(function["name"], {})[position] = kind_name(param["kind"])
    if function_kinds:
        functions = [dict(function, body=call_named(function["body"], found, function_kinds))
                     if function["kind"] == "function" else function for function in functions]
    inlines = ordered_inlines(found)
    prepared = []
    for function in functions:
        if function["kind"] == "function":
            function = dict(function, body=old_numbered(function["body"], found))
            function = named_locals(function, found)
            function = dict(function, body=folded(function["body"], found, inlines, set()))
        prepared.append(function)
    sounds = program_sounds(prepared, found)
    out = []
    for function in prepared:
        if function["kind"] == "function":
            body = named_sounds(function["body"], found, sound_banks(function, sounds), sound_flow(function, sounds))
            body = typed_literals(body, variable_kinds(function), found)
            if found["float_names"] and not shadowing(function, found["float_names"].values()):
                body = named_floats(body, found)
            function = dict(function, body=body)
        out.append(function)
    return out


# The context for a script its definition entry marks as built for an older native table, or a
# library only such scripts use. "old": groups that are current groups too (old_table_groups) are
# read as old there. "raw": every call by number without a group comes from the older, smaller
# table, also those that land on a native the header gives no group (old_direct).
def numbered(found, numbering):
    if found is None or numbering not in ("old", "raw"):
        return found
    if numbering == "raw":
        return dict(found, raw_table=True)
    return dict(found, old_groups={**found["old_groups"], **found["old_table_groups"]}, old_table=True)


def numbering_of(entry):
    return entry.get("numbering")


# The native an old-numbered call was written for. A call with an old group base selects the native
# that many numbers on (old_groups); a call by raw number where the header gives the native a group
# (shown as DIRECT) comes from an older table, and old_direct maps the numbers whose same-shaped
# twins elsewhere on the disc show what they stand for.
def meant_native(node, found):
    declared = found["by_original"].get(node["name"])
    if declared is None and re.fullmatch(r"_EXT_\d+", node["name"]):
        # a number the engine does not register, as the decompiler names it
        declared = {"no": int(node["name"][5:]), "base": None}
    if declared is None:
        return None
    if node["base"] is None and found.get("old_table") and declared["no"] in found["old_groups"]:
        # a call to an old group's first number, made without a group
        return found["by_number"].get(found["old_groups"][declared["no"]])
    if node["base"] is None:
        if (declared["base"] is None and not found.get("raw_table")) or declared["no"] not in found["old_direct"]:
            return None
        return found["by_number"].get(found["old_direct"][declared["no"]])
    if node["base"] not in found["old_groups"]:
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
        elif node["kind"] == "native":
            notes = []
            if meant_native(node, found):
                notes.append("old numbering, meant as " + meant_native(node, found))
            if node["name"].startswith("_EXT_"):
                # the engine's table has no function at this number; the VM prints "not found ext"
                # and goes on (CRunScript::ext, runscript.cpp)
                notes.append("no native at this number, so the call does nothing")
            if notes:
                node = dict(node, comment="; ".join(notes + ([node["comment"]] if node.get("comment") else [])))
        out.append(node)
    return out


# The kind of sound entry play_se plays from a bank id, by where the id came from.
SOUND_BANKS = {"SE_BANK_BASE": "base_se", "SE_BANK_BASE_2": "base_se", "SE_BANK_BASE_3": "base_se",
               "SE_BANK_BATTLE": "battle_se", 1: "base_se", 2: "base_se", 3: "base_se", 4: "battle_se"}


def assigned_var(node):
    if node is not None and node["kind"] == "var":
        return node["name"]
    if node is not None and node["kind"] == "addr" and node["target"]["kind"] == "var":
        return node["target"]["name"]
    return None


# Natives that take a bank id and an entry of it, by the positions of the two.
SOUND_ENTRY_ARGS = {"play_se": (0, 1), "stop_se": (0, 1), "pause_se": (0, 1), "set_se_volume": (0, 1),
                    "set_se_pan": (0, 1), "set_se_pitch": (0, 1), "get_se_status": (0, 1), "play_actor_se": (1, 2),
                    "play_actor_se_after": (2, 3)}
EVENT_PORT = "event port"


# A context with the definition set's event bank of each sound set, keyed by number.
def with_sound_sets(found, defs):
    if found is None or not defs.get("sound_set_banks"):
        return found
    return dict(found, sound_set_banks={int(number): bank for number, bank in defs["sound_set_banks"].items()})


def bank_kind(stem):
    return "se_" + stem.lower()


def literal_value(node, found):
    if node["kind"] == "int":
        return node["value"]
    if node["kind"] == "var":
        value = found["constants"].get(node["name"])
        if isinstance(value, dict) and value.get("kind") == "int":
            return value["value"]
    return None


# The bank load_sound loads: a path names its bank by file stem ("snd2/sp/sp_000.snd" is SP_000), a
# number is a sound set whose event bank it loads (sound_set_banks, from the definition set).
def loaded_bank(node, found):
    args = node["args"]
    source = args[1] if len(args) > 2 or args[1]["kind"] != "addr" else args[0]
    if source["kind"] == "string":
        stem = source["text"].replace("\\", "/").rsplit("/", 1)[-1].split(".", 1)[0]
        return bank_kind(stem) if stem else None
    number = literal_value(source, found)
    bank = found.get("sound_set_banks", {}).get(number)
    return bank_kind(bank) if bank else None


# What the sound pass learns across a program's functions: the bank a global always holds, the
# bank every call passes for a parameter, the bank a helper writes through a pointer parameter, and
# the event port's bank when the program loads only one there. A few rounds settle it, as one fact
# can lead to another (a helper's out-parameter feeding a global).
def program_sounds(functions, found):
    known = {"globals": {}, "params": {}, "outs": {}, "port": None}
    for _ in range(4):
        info = {"globals": collections.defaultdict(set), "params": collections.defaultdict(set),
                "outs": collections.defaultdict(set), "port": set()}
        for function in functions:
            if function["kind"] == "function":
                flow = dict(sound_flow(function, known), info=info)
                named_sounds(function["body"], found, sound_banks(function, known), flow)
        learned = {"globals": single_kinds(info["globals"]), "params": single_kinds(info["params"]),
                   "outs": single_kinds(info["outs"]), "port": next(iter(info["port"])) if len(info["port"]) == 1 else None}
        if learned == known:
            break
        known = learned
    return known


def single_kinds(found_sets):
    return {key: next(iter(kinds)) for key, kinds in found_sets.items() if len(kinds) == 1}


def sound_flow(function, known):
    local_names = {param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
    pointers = {param["name"]: position for position, param in enumerate(function["params"]) if param.get("array")}
    return {"function": function["name"], "locals": local_names, "pointers": pointers, "known": known, "info": None}


# The banks a function starts with: its globals and parameters the program pass settled, and the
# event port's.
def sound_banks(function, known):
    local_names = {param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
    banks = {name: kind for name, kind in known["globals"].items() if name not in local_names}
    for position, param in enumerate(function["params"]):
        if (function["name"], position) in known["params"]:
            banks[param["name"]] = known["params"][(function["name"], position)]
    if known["port"]:
        banks[EVENT_PORT] = known["port"]
    return banks


def set_bank(banks, name, kind, flow):
    banks[name] = kind
    info = flow.get("info") if flow else None
    if info is None:
        return
    if name == EVENT_PORT:
        info["port"].add(kind)
    elif name in flow["pointers"]:
        info["outs"][(flow["function"], flow["pointers"][name])].add(kind)
    elif name not in flow["locals"]:
        info["globals"][name].add(kind)


# The pointer parameter p an assignment writes through (p[0] = ... or *p = ...).
def pointer_target(node, flow):
    if not flow:
        return None
    if node["kind"] == "index" and node["name"] in flow["pointers"]:
        return node["name"]
    if node["kind"] == "deref" and node["operand"]["kind"] == "var" and node["operand"]["name"] in flow["pointers"]:
        return node["operand"]["name"]
    return None


# Names the entry number of play_se(id, n) and its kin by the bank the id was read from: the system
# bank (get_system_sound_id), a scene bank (get_sound_id(SE_BANK_BASE, &id)), a bank load_sound
# loaded, the event port's latest bank (get_sound_port_bank), or what program_sounds settled for a
# global, a parameter or a helper's out-parameter. banks maps each such variable to the kind of its
# entries; any other write to it forgets it. flow, when given, says which names are locals and
# pointer parameters, and collects what the function shows about globals, parameters and outputs.
def named_sounds(nodes, found, banks, flow=None):
    out = []
    known = flow["known"] if flow else {"outs": {}}
    info = flow.get("info") if flow else None
    for node in nodes:
        kind = node["kind"]
        if kind == "if":
            otherwise = None if node["else"] is None else named_sounds(node["else"], found, banks, flow)
            node = dict(node, then=named_sounds(node["then"], found, banks, flow), **{"else": otherwise})
        elif kind == "while":
            node = dict(node, body=named_sounds(node["body"], found, banks, flow))
        elif kind == "expr" and node["expr"]["kind"] == "assign":
            target, value = node["expr"]["target"], node["expr"]["value"]
            source = value["name"] if value["kind"] == "var" else None
            if pointer_target(target, flow) and source in banks:
                set_bank(banks, pointer_target(target, flow), banks[source], flow)
            elif assigned_var(target) and source in banks:
                set_bank(banks, assigned_var(target), banks[source], flow)
            else:
                banks.pop(assigned_var(target), None)
        elif kind in ("native", "call"):
            args = node["args"]
            positions = SOUND_ENTRY_ARGS.get(node["name"])
            if positions and len(args) > positions[1] and assigned_var(args[positions[0]]) in banks:
                bank, entry = positions
                named = named_literal(args[entry], banks[assigned_var(args[bank])], found)
                node = dict(node, args=args[:entry] + [named] + args[entry + 1:])
            if kind == "call":
                for position, arg in enumerate(args):
                    if info is not None and arg["kind"] == "var" and arg["name"] in banks:
                        info["params"][(node["name"], position)].add(banks[arg["name"]])
                    elif arg["kind"] == "addr" and (node["name"], position) in known["outs"]:
                        set_bank(banks, assigned_var(arg), known["outs"][(node["name"], position)], flow)
            else:
                for written in [node.get("result")] + [arg for arg in args if arg["kind"] == "addr"]:
                    banks.pop(assigned_var(written), None)
                result = assigned_var(node.get("result"))
                if node["name"] == "get_system_sound_id" and result:
                    set_bank(banks, result, "system_se", flow)
                elif node["name"] == "get_sound_id" and len(args) > 1 and assigned_var(args[1]):
                    bank = args[0]["name"] if args[0]["kind"] == "var" else args[0].get("value")
                    if bank in SOUND_BANKS:
                        set_bank(banks, assigned_var(args[1]), SOUND_BANKS[bank], flow)
                elif node["name"] == "load_sound" and len(args) >= 2:
                    loaded = loaded_bank(node, found)
                    last = args[-1]
                    target = result or (assigned_var(last) if last["kind"] == "addr" else None)
                    if target is None and flow and last["kind"] == "var" and last["name"] in flow["pointers"]:
                        target = last["name"]
                    # the two-argument form, load_sound(set, &id), loads on the event port
                    has_port = len(args) > 2 or args[1]["kind"] != "addr"
                    port = (args[0]["name"] if args[0]["kind"] == "var" else args[0].get("value")) if has_port else "SND_PORT_EVENT"
                    if loaded and target:
                        set_bank(banks, target, loaded, flow)
                    if loaded and port in ("SND_PORT_EVENT", literal_value({"kind": "var", "name": "SND_PORT_EVENT"}, found)):
                        set_bank(banks, EVENT_PORT, loaded, flow)
                elif node["name"] == "get_sound_port_bank" and result and EVENT_PORT in banks:
                    port = args[0]["name"] if args and args[0]["kind"] == "var" else None
                    if port in (None, "SND_PORT_EVENT"):
                        set_bank(banks, result, banks[EVENT_PORT], flow)
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
