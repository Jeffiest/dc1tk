import json
import re
from pathlib import Path

from c2s import compile, definitions, fingerprint, library, names, natives, parse, pathnames, sdk, source, stb

DEFAULT_NAME = re.compile(r"^(f_[0-9a-f]+|func_\d+|program_\d+|shared_\d+|start|[a-z0-9_]+_\d+_\d+)$")
DEFAULT_VARIABLE = re.compile(r"^[av]\d+$")
DEFAULT_GLOBAL = re.compile(r"^g\d+$")


def is_default_name(name, program_names):
    return bool(DEFAULT_NAME.match(name)) or (name.startswith("on_") and name[3:] in program_names)


def learned_entry(function, program_names):
    entry = {}
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
        return None, {}, {}
    expanded = library.expand(program, library.read_includes(program, path), dict(headers, **definitions.source_headers(path)))
    table = expanded["natives"] or natives.table_for(Path(path).resolve().as_posix(), rules)
    expanded["natives"] = table
    prints = fingerprint.both_fingerprints(stb.read(compile.compile_program(expanded, tables)), table)
    found = prints["exact"]
    context = sdk.context(expanded["declarations"], table, tables)
    functions = [function for function in expanded["functions"] if function["kind"] in ("function", "asm")]
    entries = {}
    for function, offset in zip(functions, sorted(found)):
        if function["kind"] != "function":
            continue
        entry = learned_entry(function, context["programs"])
        if entry:
            entries[found[offset]] = dict(entry, loose=prints["loose"][offset])
    return table, entries, script_entry(program)


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
    failures = []
    scripts = 0
    for path in source_files(source):
        try:
            table, entries, entry = script_entries(path, tables, headers, rules)
        except ValueError as error:
            failures.append("%s: %s" % (path, error))
            continue
        if table is not None:
            collected.setdefault(table, {}).update(entries)
        if entry:
            write_script(target, disc_path(path, source), entry)
            scripts += 1
    directory = Path(target) / "names"
    directory.mkdir(parents=True, exist_ok=True)
    existing = names.load(directory)
    counts = {}
    for table, entries in collected.items():
        merged = dict(existing.get(table, {}))
        for key, entry in entries.items():
            merged[key] = dict(merged.get(key, {}), **entry)
        (directory / (table + ".json")).write_text(
            json.dumps({"functions": dict(sorted(merged.items()))}, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        counts[table] = len(entries)
    return {"learned": counts, "scripts": scripts, "failures": failures}
