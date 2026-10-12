from pathlib import Path

from c2s import decompile, definitions, fingerprint, library, names, natives, stb, verify


def disc_functions(root, defs, tables):
    found = []
    for path in verify.find_scripts(root):
        table = natives.table_for(path, defs["tables"])
        prints = fingerprint.both_fingerprints(stb.read((Path(root) / path).read_bytes()), table)
        for offset, exact in prints["exact"].items():
            found.append({"path": path, "table": table, "offset": offset, "exact": exact, "loose": prints["loose"][offset]})
    return found


def check(root, defs, tables):
    functions = disc_functions(root, defs, tables)
    entries = names.merged(defs["names"])
    named_loose = {entry["loose"] for entry in entries.values() if "loose" in entry and "name" in entry}
    exact_on_disc = {function["exact"] for function in functions}
    by_table = {}
    for function in functions:
        counts = by_table.setdefault(function["table"], {"functions": 0, "exact": 0, "loose": 0, "classes": set(), "named": set()})
        counts["functions"] += 1
        counts["classes"].add(function["loose"])
        if function["exact"] in entries and "name" in entries[function["exact"]]:
            counts["exact"] += 1
            counts["named"].add(function["loose"])
        elif function["loose"] in named_loose:
            counts["loose"] += 1
            counts["named"].add(function["loose"])
    paths = set(verify.find_scripts(root))
    report = {
        "tables": {table: {"functions": counts["functions"], "named_exact": counts["exact"], "named_loose": counts["loose"],
                           "classes": len(counts["classes"]), "named_classes": len(counts["named"])}
                   for table, counts in sorted(by_table.items())},
        "stale_names": sorted(key for key in entries if key not in exact_on_disc
                              and entries[key].get("loose") not in {function["loose"] for function in functions}),
        "stale_scripts": sorted(path for path in defs["scripts"] if path not in paths),
        "scripts_documented": sum(1 for path in paths if definitions.script_entry(defs, path).get("doc")),
        "scripts": len(paths),
    }
    return report


def format_check(report):
    lines = ["%-8s %9s %9s %9s %9s" % ("table", "functions", "named", "classes", "named")]
    for table, counts in report["tables"].items():
        lines.append("%-8s %9d %9d %9d %9d" % (table, counts["functions"], counts["named_exact"] + counts["named_loose"],
                                                counts["classes"], counts["named_classes"]))
    lines.append("")
    lines.append("scripts with a doc comment: %d of %d" % (report["scripts_documented"], report["scripts"]))
    lines.append("name entries matching nothing on this disc: %d" % len(report["stale_names"]))
    lines.append("script entries for paths not on this disc: %d" % len(report["stale_scripts"]))
    return "\n".join(lines)


def todo(root, defs, tables, headers, limit):
    paths = verify.find_scripts(root)
    model = library.model_for(root, paths, tables, defs, headers)
    entries = names.with_loose(names.merged(defs["names"]), model["exact_to_loose"])
    calls = {}
    seen = {}
    for path in paths:
        table = natives.table_for(path, defs["tables"])
        program = decompile.decompile(stb.read((Path(root) / path).read_bytes()), table, tables)
        if "asm" in program:
            continue
        found = names.fingerprints(program)
        known = dict(model.get("own_names", {}), **model["class_names"])
        context = model["contexts"].get(table)
        mapping = names.assign(program, found, known, library.reserved_names(model),
                               context["program_names"] if context else {})
        for function in program["functions"]:
            for callee in names.callees(function.get("body", []), []):
                if found.get(callee):
                    loose = model["exact_to_loose"][found[callee]]
                    calls[loose] = calls.get(loose, 0) + 1
        for name, exact in found.items():
            if exact and "name" not in entries.get(exact, {}):
                loose = model["exact_to_loose"][exact]
                seen.setdefault(loose, (path, mapping[name]))
    ranked = sorted(seen, key=lambda loose: (-calls.get(loose, 0), seen[loose]))
    return [{"calls": calls.get(loose, 0), "path": seen[loose][0], "name": seen[loose][1]} for loose in ranked[:limit]], len(seen)
