import struct
from pathlib import Path

from c2s import asm, compile, decompile, definitions, library, natives, parse, sdk, source, stack, stb

STAGES = ("format", "natives", "assembly", "source")
STATUSES = ("pass", "fail", "error", "pending")


def first_difference(a, b):
    for i in range(min(len(a), len(b))):
        if a[i] != b[i]:
            return i
    return min(len(a), len(b))


def compare(original, rebuilt):
    if rebuilt == original:
        return None
    return "first difference at 0x%x (sizes 0x%x and 0x%x)" % (
        first_difference(original, rebuilt), len(original), len(rebuilt))


ERRORS = (ValueError, IndexError, KeyError, UnicodeError, struct.error)


def check_format(data, script):
    try:
        rebuilt = stb.write(script)
    except ERRORS as error:
        return {"status": "error", "detail": repr(error)}
    difference = compare(data, rebuilt)
    if difference is not None:
        return {"status": "fail", "detail": difference}
    return {"status": "pass", "coverage": stb.coverage(script)}


def check_natives(path, script, tables, rules):
    table = natives.table_for(path, rules)
    if table not in tables:
        return {"status": "pending"}
    try:
        calls = stack.native_calls(script)
    except ERRORS as error:
        return {"status": "error", "detail": repr(error)}
    known = natives.numbered(tables[table])
    unregistered = {}
    for call in calls:
        if call["no"] is None:
            return {"status": "fail", "detail": "no native number for EXT %d in function at 0x%x" % (call["index"], call["function"])}
        if call["no"] not in known:
            key = "%s:%d" % (table, call["no"])
            unregistered[key] = unregistered.get(key, 0) + 1
    return {"status": "pass", "table": table, "calls": len(calls), "unregistered": unregistered}


def fallbacks(text):
    count = 0
    for line in text.split("\n"):
        if line.strip().startswith((".op ", ".bytes ")):
            count += 1
    return count


def check_assembly(path, data, script, tables, rules):
    table = natives.table_for(path, rules)
    try:
        text = asm.disassemble(script, table, tables)
        rebuilt = asm.assemble(text, tables)
    except ERRORS as error:
        return {"status": "error", "detail": repr(error)}
    difference = compare(data, rebuilt)
    if difference is not None:
        return {"status": "fail", "detail": difference}
    return {"status": "pass", "fallbacks": fallbacks(text)}


def check_source(path, data, script, tables, model, texts, headers, rules):
    try:
        program = decompile.decompile(script, natives.table_for(path, rules), tables)
        text = source.text(library.script_program(program, path, model))
        parsed = parse.parse(text)
        rebuilt = compile.compile_program(library.expand(parsed, texts, headers), tables)
    except ERRORS as error:
        return {"status": "error", "detail": repr(error)}
    difference = compare(data, rebuilt)
    if difference is not None:
        return {"status": "fail", "detail": difference}
    includes = sum(1 for item in parsed.get("functions", []) if item["kind"] == "include")
    return {"status": "pass", "asm_functions": asm_functions(program), "lines": text.count("\n"), "includes": includes}


def asm_functions(program):
    if "asm" in program:
        return sum(1 for region in program["asm"].split("\n") if region.startswith(".function"))
    return sum(1 for function in program["functions"] if function["kind"] == "asm")


def verify_file(path, data, tables, model, texts, headers, rules):
    try:
        script = stb.read(data)
    except ERRORS as error:
        failed = {"status": "error", "detail": repr(error)}
        return {"path": path, "size": len(data), "format": failed, "natives": failed,
                "assembly": failed, "source": failed}
    return {
        "path": path,
        "size": len(data),
        "format": check_format(data, script),
        "natives": check_natives(path, script, tables, rules),
        "assembly": check_assembly(path, data, script, tables, rules),
        "source": check_source(path, data, script, tables, model, texts, headers, rules),
    }


def find_scripts(root):
    base = Path(root)
    paths = [path for path in base.rglob("*") if path.is_file() and path.suffix.lower() == ".stb"]
    return sorted(path.relative_to(base).as_posix() for path in paths)


def summarize(results):
    counts = {stage: {status: 0 for status in STATUSES} for stage in STAGES}
    for result in results:
        for stage in STAGES:
            counts[stage][result[stage]["status"]] += 1
    covered = [result["format"]["coverage"] for result in results if result["format"]["status"] == "pass"]
    calls = 0
    unregistered = {}
    fallback_lines = 0
    asm_count = 0
    source_lines = 0
    includes = sorted(result["source"].get("includes", 0) for result in results)
    for result in results:
        fallback_lines += result["assembly"].get("fallbacks", 0)
        asm_count += result["source"].get("asm_functions", 0)
        source_lines += result["source"].get("lines", 0)
    for result in results:
        outcome = result["natives"]
        if outcome["status"] == "pass":
            calls += outcome["calls"]
            for key, count in outcome["unregistered"].items():
                unregistered[key] = unregistered.get(key, 0) + count
    return {
        "scripts": len(results),
        "stages": counts,
        "coverage": sum(covered) / len(covered) if covered else 0.0,
        "native_calls": calls,
        "fallbacks": fallback_lines,
        "asm_functions": asm_count,
        "source_lines": source_lines,
        "median_includes": includes[len(includes) // 2] if includes else 0,
        "unregistered": dict(sorted(unregistered.items())),
        "ok": all(counts[stage]["fail"] == 0 and counts[stage]["error"] == 0 for stage in STAGES),
    }


def verify(root, tables, defs=None, headers=None):
    defs = defs or definitions.empty()
    paths = find_scripts(root)
    model = library.model_for(root, paths, tables, defs, headers)
    texts = library.library_texts(model)
    results = [verify_file(path, (Path(root) / path).read_bytes(), tables, model, texts, headers or {}, defs["tables"])
               for path in paths]
    report = {"summary": summarize(results), "files": results}
    report["summary"]["libraries"] = len(texts)
    report["summary"]["library_lines"] = sum(text.count("\n") for text in texts.values())
    return report


def failures(report, limit):
    lines = []
    for result in report["files"]:
        for stage in STAGES:
            outcome = result[stage]
            if outcome["status"] in ("fail", "error"):
                lines.append("%s  %s  %s: %s" % (stage, outcome["status"], result["path"], outcome["detail"]))
    return lines[:limit], len(lines)


def format_report(report, limit):
    summary = report["summary"]
    lines = ["%d scripts" % summary["scripts"], ""]
    lines.append("%-10s %8s %8s %8s %8s" % ("stage", "pass", "fail", "error", "pending"))
    for stage in STAGES:
        counts = summary["stages"][stage]
        lines.append("%-10s %8d %8d %8d %8d" % (stage, counts["pass"], counts["fail"], counts["error"], counts["pending"]))
    lines.append("")
    lines.append("format coverage: %.1f%% of bytes parsed into known structures" % (summary["coverage"] * 100))
    lines.append("assembly fallbacks: %d raw .op or .bytes lines" % summary["fallbacks"])
    lines.append("source: %d script lines, %d functions left as asm blocks" % (summary["source_lines"], summary["asm_functions"]))
    lines.append("libraries: %d files, %d lines; median script includes %d"
                 % (summary["libraries"], summary["library_lines"], summary["median_includes"]))
    unregistered = summary["unregistered"]
    lines.append("native calls: %d, of which %d use numbers the game does not register"
                 % (summary["native_calls"], sum(unregistered.values())))
    if unregistered:
        lines.append("  " + ", ".join("%s x%d" % (key, count) for key, count in unregistered.items()))
    shown, total = failures(report, limit)
    if total:
        lines.append("")
        lines.append("%d failures, first %d:" % (total, len(shown)))
        lines.extend(shown)
    lines.append("")
    lines.append("OK" if summary["ok"] else "FAILED")
    return "\n".join(lines)
