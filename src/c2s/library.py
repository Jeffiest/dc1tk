import collections
import hashlib
import re
from pathlib import Path, PurePosixPath

from c2s import assets, decompile, definitions, disc, maps, messages, names, natives, notes, parse, sdk, slots, source, stb

MINIMUM_USERS = 2


def eligible(program, found):
    usable = {}
    seen = set()
    for function in program["functions"]:
        fingerprint = found[function["name"]]
        called = names.callees(function.get("body", []), [])
        usable[function["name"]] = (fingerprint is not None and fingerprint not in seen
                                    and all(usable[name] for name in called))
        seen.add(fingerprint)
    return usable


def survey(path, program):
    if "asm" in program or any(function["kind"] == "asm" for function in program["functions"]):
        return {"path": path, "fingerprints": []}, {}
    found = names.fingerprints(program)
    usable = eligible(program, found)
    fingerprints = []
    shapes = {}
    for function in program["functions"]:
        fingerprint = found[function["name"]] if usable[function["name"]] else None
        fingerprints.append(fingerprint)
        if fingerprint is not None and fingerprint not in shapes:
            shapes[fingerprint] = dict(function, name=fingerprint, padding=None,
                                       body=names.mapped(function["body"], found))
    return {"path": path, "table": program["natives"], "fingerprints": fingerprints}, shapes


def model_for(root, paths, tables, defs, headers=None):
    files = []
    shapes = {}
    contexts = {table: sdk.with_sound_sets(sdk.header_context(table, headers or {}, tables, defs["include"]), defs)
                for table in tables}
    kind_params = {}
    script_files = {}
    exact_to_loose = {}
    script_strings = {}
    script_prints = {}
    slot_loads = {}
    asset_strings = set()
    for path in paths:
        table = natives.table_for(path, defs["tables"])
        program = decompile.decompile(stb.read((Path(root) / path).read_bytes()), table, tables)
        entry, found = survey(path, program)
        slots.survey(slot_loads, path, program)
        assets.survey(asset_strings, program, contexts.get(table))
        files.append(entry)
        exact_to_loose.update(names.loose_map(program))
        script_prints[path] = names.ordered_fingerprints(program)
        if "asm" not in program:
            script_strings[path] = notes.strings_in([function.get("body", []) for function in program["functions"]], [])
        if contexts.get(table) is not None and "asm" not in program:
            fingerprints = names.fingerprints(program)
            script_files[path] = messages.loaded_files(program["functions"], contexts[table])
            for kind, by_function in messages.kind_params(program["functions"], contexts[table]).items():
                known = kind_params.setdefault(kind, {})
                for name, positions in by_function.items():
                    if fingerprints.get(name) is not None:
                        known[fingerprints[name]] = known.get(fingerprints[name], set()) | positions
        for fingerprint, shape in found.items():
            known = shapes.setdefault(fingerprint, shape)
            if known.get("returns") == "void" and shape.get("returns") != "void":
                shapes[fingerprint] = dict(known, returns=shape["returns"])
    model = build(files, shapes, defs.get("libraries", {}))
    taken = {name for context in contexts.values() if context is not None for name in context["constants"]}
    model["asset_names"] = assets.names(asset_strings, defs["strings"], taken)
    model["contexts"] = {table: assets.in_context(context, model["asset_names"]) for table, context in contexts.items()}
    model["root"] = root
    model["kind_params"] = kind_params
    model["script_files"] = script_files
    model["language_code"] = defs["text"].get("languages", {}).get(defs["language"])
    model["disc_names"] = disc.tables(root, defs)
    model["folder_slots"] = slots.names(slot_loads, model["disc_names"].get("chara_pack", {}))
    model["message_cache"] = {}
    model["exact_to_loose"] = exact_to_loose
    model["script_prints"] = script_prints
    model["siblings"] = sibling_maps(script_strings, model["language_code"])
    model["translations"] = defs["strings"]
    model["scripts"] = defs["scripts"]
    model["triggers"] = maps.folder_triggers(root) if Path(root, "map").is_dir() else {}
    return with_globals(with_names(model, defs["names"]))


def segments(files):
    places = collections.defaultdict(list)
    for number, entry in enumerate(files):
        for position, fingerprint in enumerate(entry["fingerprints"]):
            if fingerprint is not None:
                places[fingerprint].append((number, position))
    layouts = []
    for entry in files:
        sequence = entry["fingerprints"]
        found = []
        start = 0
        while start < len(sequence):
            candidates = places.get(sequence[start], [])
            if len({number for number, _ in candidates}) < MINIMUM_USERS:
                start += 1
                continue
            end = start + 1
            while end < len(sequence) and sequence[end] is not None:
                step = end - start
                following = [(number, position) for number, position in candidates
                             if position + step < len(files[number]["fingerprints"])
                             and files[number]["fingerprints"][position + step] == sequence[end]]
                if len({number for number, _ in following}) < MINIMUM_USERS:
                    break
                candidates = following
                end += 1
            found.append({"start": start, "fingerprints": tuple(sequence[start:end])})
            start = end
        layouts.append(found)
    return layouts


def identifier(text):
    text = re.sub(r"\W", "_", text)
    return "l" + text if text[:1].isdigit() else text


def common_directory(paths):
    parts = [PurePosixPath(path).parent.parts for path in paths]
    common = []
    for column in zip(*parts):
        if len(set(column)) != 1:
            break
        common.append(column[0])
    return common


def library_fingerprint(table, fingerprints):
    return hashlib.sha1((table + ":" + ",".join(fingerprints)).encode("ascii")).hexdigest()[:16]


def build(files, shapes, library_defs=None):
    library_defs = library_defs or {}
    used_paths = set()
    layouts = segments(files)
    users = collections.defaultdict(list)
    for entry, layout in zip(files, layouts):
        for segment in layout:
            users[(entry["table"], segment["fingerprints"])].append(entry["path"])
    ranked = sorted((-len(paths), paths[0], key) for key, paths in users.items() if len(paths) >= MINIMUM_USERS)
    shared = [key for _, _, key in ranked]
    libraries = {}
    counters = collections.Counter()
    class_names = {}
    for key in shared:
        directory = common_directory(users[key])
        leaf = identifier(directory[-1]) if directory else "common"
        counters[leaf] += 1
        name = "%s_%d" % (leaf, counters[leaf])
        path = "/".join(["lib"] + directory + [name + source.EXTENSION])
        identity = library_fingerprint(key[0], key[1])
        chosen = library_defs.get(identity, {})
        wanted = chosen.get("path", "")
        if wanted.startswith("lib/") and wanted.endswith(source.EXTENSION) and wanted not in used_paths:
            path = wanted
        used_paths.add(path)
        libraries[key] = {"path": path, "name": name, "table": key[0], "fingerprints": key[1], "users": len(users[key]),
                          "user_paths": users[key], "fingerprint": identity, "doc": chosen.get("doc", ""),
                          "globals": dict(chosen.get("globals", {}))}
        for position, fingerprint in enumerate(key[1]):
            class_names.setdefault(fingerprint, "%s_%d" % (name, position + 1))
    loose = 0
    for key in users:
        for fingerprint in key[1]:
            if fingerprint not in class_names:
                loose += 1
                class_names[fingerprint] = "shared_%d" % loose
    return {"layouts": {entry["path"]: layout for entry, layout in zip(files, layouts)},
            "libraries": libraries, "class_names": class_names, "shapes": shapes}


def clashes(model, class_names):
    clashing = set()
    shared = {key[1] for key in model["libraries"]}
    for layout in model["layouts"].values():
        owners = {}
        for segment in layout:
            if segment["fingerprints"] not in shared:
                continue
            for fingerprint in segment["fingerprints"]:
                owner = owners.setdefault(class_names[fingerprint], fingerprint)
                if owner != fingerprint:
                    clashing.update((owner, fingerprint))
    return clashing


def meetings(model, clashing):
    shared = {key[1] for key in model["libraries"]}
    met = collections.defaultdict(set)
    for layout in model["layouts"].values():
        present = {fingerprint for segment in layout if segment["fingerprints"] in shared
                   for fingerprint in segment["fingerprints"] if fingerprint in clashing}
        for fingerprint in present:
            met[fingerprint] |= present - {fingerprint}
    return met


def numbered(model, class_names, clashing, taken):
    order = [fingerprint for library in model["libraries"].values() for fingerprint in library["fingerprints"]]
    order += sorted(set(class_names) - set(order))
    met = meetings(model, clashing)
    used = set(class_names.values()) | taken
    out = dict(class_names)
    copies = {}
    for fingerprint in order:
        if fingerprint not in clashing or fingerprint in copies:
            continue
        base = class_names[fingerprint]
        near = {copies[other] for other in met[fingerprint] if other in copies and class_names[other] == base}
        copy = 1
        while copy in near or (copy > 1 and "%s_%d" % (base, copy) in used):
            copy += 1
        copies[fingerprint] = copy
        if copy > 1:
            out[fingerprint] = "%s_%d" % (base, copy)
    return out


def with_names(model, table_entries):
    exact_to_loose = model.get("exact_to_loose", {})
    entries = names.with_positions(names.merged(table_entries), model.get("script_prints", {}), exact_to_loose)
    entries = names.with_loose(entries, exact_to_loose)
    class_names = dict(model["class_names"])
    for fingerprint, entry in entries.items():
        if fingerprint in class_names and "name" in entry:
            class_names[fingerprint] = entry["name"]
    taken = reserved_names(model)
    for fingerprint, name in list(class_names.items()):
        if name in taken:
            class_names[fingerprint] = model["class_names"][fingerprint]
    class_names = numbered(model, class_names, clashes(model, class_names), taken)
    for fingerprint in clashes(model, class_names):
        class_names[fingerprint] = model["class_names"][fingerprint]
    own_names = {fingerprint: entry["name"] for fingerprint, entry in entries.items()
                 if fingerprint not in class_names and "name" in entry and entry["name"] not in taken}
    return dict(model, class_names=class_names, entries=entries, own_names=own_names)


def library_slots(library, model):
    return names.global_uses([model["shapes"][fingerprint] for fingerprint in library["fingerprints"]])


def library_variables(library, model):
    found = set()
    for fingerprint in library["fingerprints"]:
        entry = model["entries"].get(fingerprint, {})
        found |= set(entry.get("params", [])) | set(entry.get("locals", []))
    return found


def conflicting(used, slots, chosen):
    dropped = set()
    by_name = collections.defaultdict(set)
    for library in used:
        for slot in slots[library["path"]]:
            by_name[chosen[library["path"]].get(slot)].add(slot)
    for slot in set().union(*(slots[library["path"]] for library in used)):
        touching = [library for library in used if slot in slots[library["path"]]]
        wanted = {chosen[library["path"]].get(slot) for library in touching}
        if len(wanted) > 1 or any(name is not None and len(by_name[name]) > 1 for name in wanted):
            dropped |= {(library["path"], slot) for library in touching if slot in chosen[library["path"]]}
    return dropped


def with_globals(model):
    libraries = list(model["libraries"].values())
    taken = reserved_names(model) | set(model["class_names"].values()) | set(model["own_names"].values())
    slots = {library["path"]: library_slots(library, model) for library in libraries}
    chosen = {}
    for library in libraries:
        variables = library_variables(library, model)
        chosen[library["path"]] = {slot: name for slot, name in library["globals"].items()
                                   if slot in slots[library["path"]] and names.IDENTIFIER.match(name)
                                   and not names.GLOBAL_NAME.match(name) and name not in taken | variables}
    users = collections.defaultdict(list)
    for library in libraries:
        for path in library["user_paths"]:
            users[path].append(library)
    dropped = True
    while dropped:
        dropped = set()
        for used in users.values():
            dropped |= conflicting(used, slots, chosen)
        for path, slot in dropped:
            chosen[path].pop(slot, None)
    return dict(model, libraries={key: dict(library, globals=chosen[library["path"]])
                                  for key, library in model["libraries"].items()})


def reserved_names(model):
    taken = set()
    for found in model.get("contexts", {}).values():
        taken |= sdk.reserved(found)
    return taken


def library_text(library, model):
    class_names = model["class_names"]
    functions = []
    for fingerprint in library["fingerprints"]:
        shape = model["shapes"][fingerprint]
        body = names.renamed_variables(names.mapped(shape["body"], class_names), library.get("globals", {}))
        function = dict(shape, name=class_names[fingerprint], body=body)
        if fingerprint in model["entries"]:
            function = names.with_variables(function, model["entries"][fingerprint], reserved_names(model))
        functions.append(function)
    found = model["contexts"].get(library["table"])
    header = sdk.header_name(library["table"]) if found is not None else None
    params = {kind: {class_names[fingerprint]: positions[fingerprint] for fingerprint in class_names if fingerprint in positions}
              for kind, positions in model["kind_params"].items()}
    functions = messages.annotate_functions(functions, found, params, library_messages(library, model), model["disc_names"])
    functions = notes.annotate_siblings(functions, library_siblings(library, model))
    # a library only scripts built for one older native table use reads its calls the same way
    numberings = {sdk.numbering_of(definitions.script_entry({"scripts": model["scripts"]}, path))
                  for path in library["user_paths"]}
    if len(numberings) == 1:
        found = sdk.numbered(found, numberings.pop())
    presented = notes.annotate_functions(sdk.present_functions(functions, found, params), model["translations"])
    return source.library_text(library["table"], presented, header, library["doc"])


def sibling_maps(script_strings, code):
    if code is None:
        return {}
    found = {}
    for path, strings in script_strings.items():
        match = definitions.VARIANT.search(path)
        if match is None or match.group(0) == "_%d.stb" % code:
            continue
        sibling = definitions.VARIANT.sub("_%d.stb" % code, path)
        if sibling in script_strings:
            pairs = notes.sibling_strings(strings, script_strings[sibling])
            if pairs:
                found[path] = pairs
    return found


def library_siblings(library, model):
    agreed = {}
    clashing = set()
    for path in library["user_paths"]:
        for text, english in model["siblings"].get(path, {}).items():
            if agreed.setdefault(text, english) != english:
                clashing.add(text)
    return {text: english for text, english in agreed.items() if text not in clashing}


def script_messages(path, model):
    if model.get("language_code") is None:
        return {}
    return messages.texts_for(model["script_files"].get(path, []), model["root"], model["language_code"],
                              model["message_cache"])


def library_messages(library, model):
    agreed = {}
    clashing = set()
    for path in library["user_paths"]:
        for number, text in script_messages(path, model).items():
            if agreed.setdefault(number, text) != text:
                clashing.add(number)
    return {number: text for number, text in agreed.items() if number not in clashing}


def library_texts(model):
    return {library["path"]: library_text(library, model) for library in model["libraries"].values()}


def script_program(program, path, model):
    if "asm" in program or any(function["kind"] == "asm" for function in program["functions"]):
        return program
    context = slots.in_folder(model["contexts"].get(program["natives"]), path, model.get("folder_slots", {}))
    program_names = context["program_names"] if context is not None else {}
    found = names.fingerprints(program)
    known = dict(model.get("own_names", {}), **model["class_names"])
    mapping = names.assign(program, found, known, reserved_names(model), program_names)
    renamed = names.described(names.rename(program, mapping),
                              [found[function["name"]] for function in program["functions"]], model["entries"],
                              reserved_names(model))
    params = {kind: {mapping[name]: positions[fingerprint] for name, fingerprint in found.items() if fingerprint in positions}
              for kind, positions in model["kind_params"].items()}
    renamed = dict(renamed, functions=messages.annotate_functions(renamed["functions"], context, params,
                                                                   script_messages(path, model), model["disc_names"]))
    renamed = dict(renamed, functions=notes.annotate_siblings(renamed["functions"], model["siblings"].get(path, {})))
    included = {}
    forced = {}
    for segment in model["layouts"].get(path, []):
        library = model["libraries"].get((program["natives"], segment["fingerprints"]))
        span = range(segment["start"], segment["start"] + len(segment["fingerprints"]))
        originals = [program["functions"][position]["name"] for position in span]
        if library is not None and all(mapping[name] == model["class_names"][found[name]] for name in originals):
            included[segment["start"]] = (library, len(span))
            forced.update(library.get("globals", {}))
    functions = []
    padding_of = {}
    hidden = []
    position = 0
    while position < len(renamed["functions"]):
        if position not in included:
            functions.append(renamed["functions"][position])
            position += 1
            continue
        library, count = included[position]
        functions.append({"kind": "include", "path": library["path"], "system": False})
        for function in renamed["functions"][position:position + count]:
            hidden.append(function)
            if function["padding"] is not None:
                padding_of[function["name"]] = function["padding"]
        position += count
    split = dict(renamed, functions=functions, padding_of=padding_of)
    entry = definitions.script_entry({"scripts": model["scripts"]}, path)
    scripted = names.with_script(split, entry, names.global_uses(hidden), forced)
    presented = sdk.present(scripted, sdk.numbered(context, sdk.numbering_of(entry)), params)
    triggers = model.get("triggers", {}).get(str(Path(path).parent.as_posix()), {})
    if triggers:
        programs = [dict(entry, note="; ".join(triggers[original["no"]])) if original["no"] in triggers else entry
                    for entry, original in zip(presented["programs"], program["programs"])]
        presented = dict(presented, programs=programs)
    return notes.annotate(presented, model["translations"])


def absorb(path, headers, declarations, seen):
    return sdk.header_declarations(path, headers, declarations, seen)


def expand(program, texts, headers=None):
    if "asm" in program:
        return program
    headers = headers or {}
    state = {
        "declarations": {key: dict(value) if isinstance(value, dict) else list(value)
                         for key, value in dict(sdk.EMPTY, **program.get("declarations", {})).items()},
        "absorbed": {}, "seen": set(), "included": set(), "texts": texts, "headers": headers,
        "functions": [], "table": program["natives"],
    }
    for item in program["functions"]:
        if item["kind"] != "include":
            state["functions"].append(item)
        elif item["system"]:
            header_table = absorbed_header(item["path"], state)
            state["table"] = state["table"] or header_table
        else:
            included_table = inline_library(item["path"], state, [])
            if state["table"] is not None and included_table is not None and included_table != state["table"]:
                raise ValueError("%s uses natives %s, not %s" % (item["path"], included_table, state["table"]))
    functions = state["functions"]
    padding_of = program.get("padding_of", {})
    defined = {function["name"] for function in functions}
    for name in padding_of:
        if name not in defined:
            raise ValueError("#pragma padding_of names unknown function " + name)
    functions = [dict(function, padding=padding_of[function["name"]]) if function["name"] in padding_of else function
                 for function in functions]
    return dict(program, natives=state["table"], functions=functions, padding_of={}, declarations=state["declarations"],
                library_docs=state.get("library_docs", {}))


def absorbed_header(path, state):
    if path not in state["absorbed"]:
        state["absorbed"][path] = absorb(path, state["headers"], state["declarations"], state["seen"])
    return state["absorbed"][path]


# Puts a library's functions where it is included, after the libraries it includes itself. Each
# library is included once however many files include it, as if every library had #pragma once.
# A library's own #defines, enums, typedefs and helpers become the script's, as with a C header.
def inline_library(path, state, chain):
    if path in chain:
        raise ValueError("libraries include each other: " + " -> ".join(chain + [path]))
    if path in state["included"]:
        return None
    if path not in state["texts"]:
        raise ValueError("cannot find included file " + path)
    state["included"].add(path)
    included = parse.parse(state["texts"][path])
    state.setdefault("library_docs", {})[path] = included.get("doc", "")
    for key, value in included.get("declarations", {}).items():
        if isinstance(value, dict):
            state["declarations"].setdefault(key, {}).update(value)
        else:
            state["declarations"].setdefault(key, []).extend(value)
    table = included["natives"]
    for sub in included["functions"]:
        if sub["kind"] == "include" and sub["system"]:
            header_table = absorbed_header(sub["path"], state)
            table = table or header_table
        elif sub["kind"] == "include":
            nested = inline_library(sub["path"], state, chain + [path])
            if table is not None and nested is not None and nested != table:
                raise ValueError("%s uses natives %s, not %s" % (sub["path"], nested, table))
        else:
            # which library a function came from, for defs learn
            state["functions"].append(dict(sub, library=path))
    return table




# The texts of a script's library includes and of the libraries they include, looked up from the
# script's folder and each folder above it, then in the SDK folder, whose libraries (modkit/...) any
# script or mod can include. overlay, keyed by include path, stands in for files on disk
# (build --mod).
def read_includes(program, source_path, overlay=None, sdk_dir=None):
    texts = {}
    directory = Path(source_path).resolve().parent
    pending = [item["path"] for item in program.get("functions", []) if item["kind"] == "include" and not item["system"]]
    while pending:
        path = pending.pop()
        if path in texts:
            continue
        text = overlay.get(path) if overlay else None
        if text is None:
            for folder in [directory] + list(directory.parents) + [Path(sdk_dir or sdk.SDK_DIR)]:
                candidate = folder / path
                if candidate.is_file():
                    text = candidate.read_text(encoding="utf-8")
                    break
        if text is None:
            continue
        texts[path] = text
        try:
            nested = parse.parse(text)
        except ValueError:
            continue
        pending += [item["path"] for item in nested.get("functions", []) if item["kind"] == "include" and not item["system"]]
    return texts
