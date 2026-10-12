import json
import re
from pathlib import Path

def mapped(node, mapping):
    if isinstance(node, list):
        return [mapped(item, mapping) for item in node]
    if not isinstance(node, dict):
        return node
    out = {key: mapped(value, mapping) for key, value in node.items()}
    if node.get("kind") == "call":
        out["name"] = mapping[node["name"]]
    return out


def callees(node, out):
    if isinstance(node, list):
        for item in node:
            callees(item, out)
    elif isinstance(node, dict):
        if node.get("kind") == "call":
            out.append(node["name"])
        for value in node.values():
            callees(value, out)
    return out


def fingerprints(program):
    known = program.get("fingerprints", {})
    return {function["name"]: known.get(function["name"]) if function["kind"] == "function" else None
            for function in program["functions"]}


IDENTIFIER = re.compile(r"^[A-Za-z][A-Za-z0-9_]*$")


def load(directory):
    tables = {}
    for path in sorted(Path(directory).glob("*.json")):
        functions = json.loads(path.read_text(encoding="utf-8"))["functions"]
        for fingerprint, entry in functions.items():
            for name in [entry.get("name", "x")] + entry.get("params", []) + entry.get("locals", []):
                if not IDENTIFIER.match(name):
                    raise ValueError("%s: %s is not a valid name for %s" % (path.name, name, fingerprint))
        tables[path.stem] = functions
    return tables


def ordered_fingerprints(program):
    exact = program.get("fingerprints", {})
    return [exact.get(function["name"]) for function in program.get("functions", [])]


def loose_map(program):
    exact = program.get("fingerprints", {})
    loose = program.get("loose_fingerprints", {})
    return {exact[name]: loose[name] for name in exact if name in loose}


def with_loose(entries, exact_to_loose):
    index = {}
    ambiguous = set()
    for entry in entries.values():
        if "loose" in entry and "name" in entry:
            known = index.setdefault(entry["loose"], entry)
            if known["name"] != entry["name"]:
                ambiguous.add(entry["loose"])
    out = dict(entries)
    for exact, loose in exact_to_loose.items():
        if exact not in out and loose in index and loose not in ambiguous:
            out[exact] = index[loose]
    return out


def with_positions(entries, script_prints, exact_to_loose):
    groups = {}
    for path, prints in script_prints.items():
        groups.setdefault(re.sub(r"_\d\.stb$", ".stb", path), []).append(prints)
    out = dict(entries)
    for variants in groups.values():
        for column in zip(*variants) if len({len(prints) for prints in variants}) == 1 else []:
            named = {exact_to_loose.get(exact): entries[exact] for exact in column if exact in entries and "name" in entries[exact]}
            for exact in column:
                loose = exact_to_loose.get(exact)
                if exact not in out and loose in named:
                    out[exact] = named[loose]
    return out


def merged(table_entries):
    found = {}
    for table in table_entries.values():
        found.update(table)
    return found


def variable_mapping(function, entry, reserved=frozenset()):
    wanted = list(zip(function["params"], entry.get("params", []))) + list(zip(function["locals"], entry.get("locals", [])))
    kept = {param["name"] for param in function["params"]} | {declared["name"] for declared in function["locals"]}
    kept = (kept | set(variables(function["body"], []))) - {declared["name"] for declared, _ in wanted}
    mapping = {}
    taken = kept | set(callees(function["body"], []))
    for declared, name in wanted:
        base = name + "_value" if name in reserved else name
        name = base
        copy = 1
        while name in taken:
            copy += 1
            name = "%s_%d" % (base, copy)
        taken.add(name)
        mapping[declared["name"]] = name
    return mapping


def variables(node, out):
    if isinstance(node, list):
        for item in node:
            variables(item, out)
    elif isinstance(node, dict):
        if node.get("kind") in ("var", "index"):
            out.append(node["name"])
        for value in node.values():
            variables(value, out)
    return out


def renamed_variables(node, mapping):
    if isinstance(node, list):
        return [renamed_variables(item, mapping) for item in node]
    if not isinstance(node, dict):
        return node
    out = {key: renamed_variables(value, mapping) for key, value in node.items()}
    if node.get("kind") in ("var", "index") and node["name"] in mapping:
        out["name"] = mapping[node["name"]]
    return out


def with_variables(function, entry, reserved=frozenset()):
    if entry.get("doc"):
        function = dict(function, doc=entry["doc"])
    mapping = variable_mapping(function, entry, reserved)
    if not mapping:
        return function
    return dict(function,
                params=[dict(param, name=mapping.get(param["name"], param["name"])) for param in function["params"]],
                locals=[dict(declared, name=mapping.get(declared["name"], declared["name"])) for declared in function["locals"]],
                body=renamed_variables(function["body"], mapping))


def described(program, found, entries, reserved=frozenset()):
    functions = []
    for function, fingerprint in zip(program["functions"], found):
        if function["kind"] == "function" and fingerprint in entries:
            function = with_variables(function, entries[fingerprint], reserved)
        functions.append(function)
    return dict(program, functions=functions)


def entry_names(program, program_names):
    found = {}
    for entry in program["programs"]:
        if entry["no"] in program_names:
            found.setdefault(entry["function"], "on_" + program_names[entry["no"]])
        else:
            found.setdefault(entry["function"], "program_%d" % entry["no"])
    found.setdefault(program["main"], "start")
    return found


def assign(program, found, class_names, reserved=frozenset(), program_names=None):
    entries = entry_names(program, program_names or {})
    handler_names = {"on_" + name for name in (program_names or {}).values()}
    mapping = {}
    taken = set(reserved)
    own = 0
    for function in program["functions"]:
        fingerprint = found[function["name"]]
        if fingerprint is not None and fingerprint in class_names:
            base = class_names[fingerprint]
            # A handler named for its program elsewhere (on_dodge_shot) takes the name of the
            # program it handles here, as the same code can be bound to another one.
            if base in handler_names and entries.get(function["name"], base) in handler_names:
                base = entries[function["name"]] if function["name"] in entries else base
        elif function["name"] in entries:
            base = entries[function["name"]]
        else:
            own += 1
            base = "func_%d" % own
        if base in taken and function["name"] in entries:
            base = entry_alias(base, entries[function["name"]])
        name = base
        copy = 1
        while name in taken:
            copy += 1
            name = "%s_%d" % (base, copy)
        taken.add(name)
        mapping[function["name"]] = name
    return mapping


def entry_alias(base, entry):
    if entry.startswith("program_"):
        return "%s_for_%s" % (base, entry)
    return entry


def rename(program, mapping):
    functions = []
    for function in program["functions"]:
        renamed = dict(function, name=mapping[function["name"]])
        if function["kind"] == "function":
            renamed["body"] = mapped(function["body"], mapping)
        functions.append(renamed)
    programs = [{"no": entry["no"], "function": mapping[entry["function"]]} for entry in program["programs"]]
    return dict(program, functions=functions, programs=programs, main=mapping[program["main"]])


GLOBAL_NAME = re.compile(r"^g\d+$")


def global_mapping(program, entry, protected=frozenset(), forced=None):
    forced = forced or {}
    mapping = {declared["name"]: forced[declared["name"]] for declared in program["globals"] if declared["name"] in forced}
    taken = set(mapping.values())
    for declared, wanted in zip(program["globals"], entry.get("globals", [])):
        name = wanted if isinstance(wanted, str) else wanted["name"]
        if declared["name"] in protected or declared["name"] in mapping or name in taken or not IDENTIFIER.match(name):
            continue
        mapping[declared["name"]] = name
        taken.add(name)
    return mapping


def with_script(program, entry, protected=frozenset(), forced=None):
    if "asm" in program or not (entry or forced):
        return program
    mapping = global_mapping(program, entry, protected, forced)
    functions = [dict(function, body=renamed_variables(function["body"], mapping)) if function["kind"] == "function"
                 else function for function in program["functions"]]
    globals_ = [dict(declared, name=mapping.get(declared["name"], declared["name"])) for declared in program["globals"]]
    out = dict(program, functions=functions, globals=globals_)
    if entry.get("doc"):
        out["doc"] = entry["doc"]
    return out


def global_uses(functions):
    return {name for function in functions if function["kind"] == "function"
            for name in variables(function["body"], []) if GLOBAL_NAME.match(name)}


def apply(program, entries, reserved=frozenset(), program_names=None):
    if "asm" in program or any(function["kind"] == "asm" for function in program["functions"]):
        return program
    found = fingerprints(program)
    class_names = {fingerprint: entry["name"] for fingerprint, entry in entries.items()
                   if "name" in entry and entry["name"] not in reserved}
    renamed = rename(program, assign(program, found, class_names, reserved, program_names))
    return described(renamed, [found[function["name"]] for function in program["functions"]], entries, reserved)
