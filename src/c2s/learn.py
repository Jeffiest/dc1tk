import json
import re
from pathlib import Path

from c2s import compile, decompile, definitions, fingerprint, library, names, natives, parse, pathnames, sdk, source, stb

DEFAULT_NAME = re.compile(r"^(f_[0-9a-f]+|func_\d+|program_\d+|shared_\d+|start|[a-z0-9_]+_\d+_\d+)$")
DEFAULT_VARIABLE = re.compile(r"^[av]\d+$")
DEFAULT_GLOBAL = re.compile(r"^g\d+$")


def is_default_name(name, program_names):
    return bool(DEFAULT_NAME.match(name)) or (name.startswith("on_") and name[3:] in program_names)


BASE_TYPES = ("int", "float", "string")


def learned_entry(function, program_names, inferred=None):
    entry = {}
    # a value kind or enum a variable is declared with
    types = {variable["name"]: variable["type"] for variable in function["params"] + function["locals"]
             if variable["type"] not in BASE_TYPES}
    if types:
        entry["types"] = types
    # a return type the decompiler would not have chosen
    if inferred is not None and function.get("returns") in ("void", "int", "float") and function["returns"] != inferred:
        entry["returns"] = function["returns"]
    if not is_default_name(function["name"], program_names):
        entry["name"] = function["name"]
    params = [param["name"] for param in function["params"]]
    if any(not DEFAULT_VARIABLE.match(name) for name in params):
        entry["params"] = params
    locals_ = [declared["name"] for declared in function["locals"]]
    if any(not DEFAULT_VARIABLE.match(name) for name in locals_):
        entry["locals"] = locals_
    if function.get("doc"):
        entry["doc"] = function["doc"]
    return entry


def script_entry(program):
    entry = {}
    globals_ = [declared["name"] for declared in program["globals"]]
    if any(not DEFAULT_GLOBAL.match(name) for name in globals_):
        entry["globals"] = globals_
    if program.get("doc"):
        entry["doc"] = program["doc"]
    return entry


def script_entries(path, tables, headers, rules):
    program = parse.parse(Path(path).read_text(encoding="utf-8"))
    if "asm" in program:
        return None, {}, {}, {}
    expanded = library.expand(program, library.read_includes(program, path), dict(headers, **definitions.source_headers(path)))
    table = expanded["natives"] or natives.table_for(Path(path).resolve().as_posix(), rules)
    expanded["natives"] = table
    notes = {}
    script = stb.read(compile.compile_program(expanded, tables, notes))
    prints = fingerprint.both_fingerprints(script, table)
    found = prints["exact"]
    decompiled = decompile.decompile(script, table, tables)
    inferred = [function.get("returns") for function in decompiled.get("functions", [])
                if function["kind"] in ("function", "asm")] if "asm" not in decompiled else []
    context = sdk.context(expanded["declarations"], table, tables)
    functions = [function for function in expanded["functions"] if function["kind"] in ("function", "asm")]
    entries = {}
    seen = set()
    for position, (function, offset) in enumerate(zip(functions, sorted(found))):
        if function["kind"] != "function":
            continue
        # A function the file holds twice decompiles once under its name and again under an alias
        # (start_event_for_program_506, name_2); only the first copy says what to call it.
        if found[offset] in seen:
            continue
        seen.add(found[offset])
        entry = learned_entry(function, context["programs"], inferred[position] if position < len(inferred) else None)
        if notes.get(function["name"]):
            entry["notes"] = notes[function["name"]]
        if entry:
            entries[found[offset]] = dict(entry, loose=prints["loose"][offset])
    slots = {}
    if "asm" not in decompiled and len(decompiled["globals"]) == len(expanded["globals"]):
        slots = {declared["name"]: shown["name"] for declared, shown in zip(expanded["globals"], decompiled["globals"])}
    return table, entries, script_entry(program), library_entries(functions, found, table, expanded["library_docs"], slots)


# The libraries a script includes, keyed as the decompiler keys them (the fingerprints of the
# functions each holds, in order), with the path the script includes them by, their doc and the
# names of the globals they use, by the slot the decompiler names them for (g0). slots maps each
# global's name to that slot name.
def library_entries(functions, found, table, docs, slots=None):
    runs = []
    for function, offset in zip(functions, sorted(found)):
        path = function.get("library")
        if path is None:
            continue
        if runs and runs[-1][0] == path:
            runs[-1][1].append(found[offset])
            runs[-1][2].append(function)
        else:
            runs.append((path, [found[offset]], [function]))
    out = {}
    for path, prints, members in runs:
        entry = {"path": path}
        if docs.get(path):
            entry["doc"] = docs[path]
        used = {name for function in members for name in names.variables(function["body"], [])
                if name not in {variable["name"] for variable in function["params"] + function["locals"]}}
        globals_ = {slots[name]: name for name in used if name in (slots or {}) and not DEFAULT_GLOBAL.match(name)}
        if globals_:
            entry["globals"] = dict(sorted(globals_.items(), key=lambda item: int(item[0][1:])))
        out[library.library_fingerprint(table, prints)] = entry
    return out


ALIAS = re.compile(r"^(.+?)(?:_for_program_\d+|_\d+)$")


# One entry from the copies of a function in several scripts. For each field a copy that differs
# from what the set already says is an edit and wins; otherwise the first copy's. A name that is
# another copy's name plus the suffix the decompiler adds when that name is taken in a file
# (_for_program_506, _2) is an alias, not a name.
def resolved(copies, existing):
    out = {}
    for field in dict.fromkeys(field for copy in copies for field in copy):
        values = [copy[field] for copy in copies if field in copy]
        if field == "name":
            plain = [value for value in values if not (ALIAS.match(value) and ALIAS.match(value).group(1) in values)]
            values = plain or values
        edited = [value for value in values if value != existing.get(field)]
        out[field] = (edited or values)[0]
    return out


def source_files(tree):
    tree = Path(tree)
    if tree.is_file():
        return [tree]
    return sorted(path for path in tree.rglob("*" + source.EXTENSION) if "lib" not in path.relative_to(tree).parts[:1])


def disc_path(path, source):
    source = Path(source)
    relative = Path(path).name if source.is_file() else pathnames.unnamed(source, Path(path).relative_to(source).as_posix())
    return str(Path(relative).with_suffix(".stb").as_posix())


def write_script(target, path, entry):
    file = Path(target) / "scripts" / Path(path).with_suffix(".json")
    existing = json.loads(file.read_text(encoding="utf-8")) if file.is_file() else {}
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(json.dumps(dict(existing, **entry), indent=1, ensure_ascii=False) + "\n", encoding="utf-8")


def learn(source, target, tables, headers, rules):
    collected = {}
    found_libraries = {}
    failures = []
    scripts = 0
    for path in source_files(source):
        try:
            table, entries, entry, libraries = script_entries(path, tables, headers, rules)
        except ValueError as error:
            failures.append("%s: %s" % (path, error))
            continue
        if table is not None:
            copies = collected.setdefault(table, {})
            for key, function_entry in entries.items():
                copies.setdefault(key, []).append(function_entry)
        if entry:
            write_script(target, disc_path(path, source), entry)
            scripts += 1
        for key, library_entry in libraries.items():
            # the set keeps library paths under the disc's folder names, as the decompiler makes them
            if Path(source).is_dir():
                library_entry = dict(library_entry, path=pathnames.unnamed(source, library_entry["path"]))
            found_libraries.setdefault(key, library_entry)
    directory = Path(target) / "names"
    directory.mkdir(parents=True, exist_ok=True)
    existing = names.load(directory)
    counts = {}
    for table, entries in collected.items():
        merged = dict(existing.get(table, {}))
        for key, copies in entries.items():
            merged[key] = dict(merged.get(key, {}), **resolved(copies, merged.get(key, {})))
        (directory / (table + ".json")).write_text(
            json.dumps({"functions": dict(sorted(merged.items()))}, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        counts[table] = len(entries)
    libraries = write_libraries(target, found_libraries)
    return {"learned": counts, "scripts": scripts, "libraries": libraries, "failures": failures}


# Records the path, doc and global names of each library the set already knows (a key the
# decompiler made), when they differ from what the set says. Returns how many changed.
def write_libraries(target, found):
    file = Path(target) / "libraries.json"
    if not file.is_file():
        return 0
    data = json.loads(file.read_text(encoding="utf-8"))
    known = data.get("libraries", {})
    taken = {entry.get("path") for entry in known.values()}
    changed = 0
    for key, entry in found.items():
        if key not in known:
            continue
        updated = dict(known[key])
        if entry["path"] != updated.get("path") and entry["path"] not in taken:
            taken.discard(updated.get("path"))
            updated["path"] = entry["path"]
            taken.add(entry["path"])
        if entry.get("doc") and entry["doc"] != updated.get("doc"):
            updated["doc"] = entry["doc"]
        if entry.get("globals"):
            updated["globals"] = dict(updated.get("globals", {}), **entry["globals"])
        if updated != known[key]:
            known[key] = updated
            changed += 1
    if changed:
        file.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    return changed
