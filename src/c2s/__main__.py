import argparse
import json
import sys
from pathlib import Path

from c2s import asm, compile, coverage, decompile, definitions, disc, extract, learn, library, messages, names, natives, notes, parse, rebuild, sdk, source, stb, verify


def run_extract(args):
    manifest = extract.extract(args.iso, args.out)
    packed = sum(1 for item in manifest["files"] if item["packed"])
    print("%d files written to %s (%d from packs)" % (len(manifest["files"]), args.out, packed))
    print("%d scripts listed in %s" % (len(manifest["scripts"]), Path(args.out) / "manifest.json"))
    return 0


def run_natives(args):
    tables = natives.generate(args.chronicletwo, args.out)
    for name, table in tables.items():
        print("%-8s %4d natives  %s" % (name, len(table["natives"]), Path(args.out) / (name + ".json")))
    return 0


def loaded_definitions(args):
    if args.defs == ["none"]:
        return definitions.empty()
    return definitions.load(args.defs or [definitions.DEFAULT_SET])


def chosen_table(args, path, defs):
    return args.table or natives.table_for(Path(path).resolve().as_posix(), defs["tables"])


def run_disassemble(args):
    defs = loaded_definitions(args)
    text = asm.disassemble(stb.read(Path(args.stb).read_bytes()), chosen_table(args, args.stb, defs), natives.load(args.natives))
    write_output(args.output, text)
    return 0


def run_assemble(args):
    text = Path(args.source).read_text(encoding="utf-8")
    Path(args.output).write_bytes(asm.assemble(text, natives.load(args.natives)))
    return 0


def write_output(path, text):
    if path:
        Path(path).write_text(text, encoding="utf-8")
    else:
        sys.stdout.write(text)


def run_decompile(args):
    tables = natives.load(args.natives)
    defs = loaded_definitions(args)
    headers = definitions.header_texts(defs, args.sdk)
    if Path(args.stb).is_dir():
        return run_decompile_disc(args, tables, defs, headers)
    table = chosen_table(args, args.stb, defs)
    program = decompile.decompile(stb.read(Path(args.stb).read_bytes()), table, tables)
    found = sdk.header_context(table, headers, tables, defs["include"])
    program_names = found["program_names"] if found is not None else {}
    entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
    named = names.apply(program, entries, sdk.reserved(found), program_names)
    named = with_messages(named, found, defs, args.stb)
    scripted = names.with_script(named, definitions.script_entry(defs, args.stb))
    params = messages.kind_params(named["functions"], found) if found is not None and "asm" not in named else {}
    write_output(args.output, source.text(notes.annotate(sdk.present(scripted, found, params), defs["strings"])))
    return 0


def disc_root(path):
    for folder in Path(path).resolve().parents:
        if (folder / "manifest.json").is_file():
            return folder
    return None


def with_messages(program, found, defs, path):
    root = disc_root(path)
    code = defs["text"].get("languages", {}).get(defs["language"])
    if found is None or root is None or code is None or "asm" in program:
        return program
    texts = messages.texts_for(messages.loaded_files(program["functions"], found), root, code, {})
    params = messages.kind_params(program["functions"], found)
    lookups = disc.tables(root, defs)
    functions = messages.annotate_functions(program["functions"], found, params, texts, lookups)
    return dict(program, functions=notes.annotate_siblings(functions, sibling_strings(program, path, code, found)))


def sibling_strings(program, path, code, found):
    match = definitions.VARIANT.search(Path(path).name)
    sibling = Path(path).with_name(definitions.VARIANT.sub("_%d.stb" % code, Path(path).name))
    if match is None or match.group(0) == "_%d.stb" % code or not sibling.is_file():
        return {}
    other = decompile.decompile(stb.read(sibling.read_bytes()), found["table"], natives.load())
    if "asm" in other:
        return {}
    mine = notes.strings_in([function.get("body", []) for function in program["functions"]], [])
    theirs = notes.strings_in([function.get("body", []) for function in other["functions"]], [])
    return notes.sibling_strings(mine, theirs)


def run_decompile_disc(args, tables, defs, headers):
    if not args.output:
        raise SystemExit("decompiling a directory needs -o OUT_DIR")
    out = Path(args.output)
    root = disc_root(Path(args.stb) / "manifest.json") or Path(args.stb)
    prefix = Path(args.stb).resolve().relative_to(root.resolve()).as_posix()
    paths = [path if prefix == "." else prefix + "/" + path for path in verify.find_scripts(args.stb)]
    defs, headers, generated = disc.with_header(str(root), defs, headers)
    if generated is not None:
        (out / "include").mkdir(parents=True, exist_ok=True)
        (out / "include" / disc.HEADER).write_text(generated, encoding="utf-8")
    model = library.model_for(str(root), paths, tables, defs, headers)
    texts = library.library_texts(model)
    for path, text in texts.items():
        (out / path).parent.mkdir(parents=True, exist_ok=True)
        (out / path).write_text(text, encoding="utf-8")
    for path in paths:
        table = natives.table_for(path, defs["tables"])
        program = decompile.decompile(stb.read((root / path).read_bytes()), table, tables)
        target = (out / path).with_suffix(".c2s")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source.text(library.script_program(program, path, model)), encoding="utf-8")
    print("%d scripts and %d library files written to %s" % (len(paths), len(texts), out))
    return 0


def run_setup(args):
    extracted = Path(args.extracted)
    if not (extracted / "manifest.json").is_file():
        manifest = extract.extract(args.iso, str(extracted))
        print("%d files extracted to %s" % (len(manifest["files"]), extracted))
    else:
        extract.extract_boot(args.iso, str(extracted))
    args.stb = str(extracted)
    args.table = None
    return run_decompile(args)


def run_compile(args):
    defs = loaded_definitions(args)
    program = parse.parse(Path(args.source).read_text(encoding="utf-8"))
    if "asm" not in program and program["natives"] is None:
        program["natives"] = args.table
    headers = dict(definitions.header_texts(defs, args.sdk), **definitions.source_headers(args.source))
    program = library.expand(program, library.read_includes(program, args.source), headers)
    if "asm" not in program and program["natives"] is None:
        program["natives"] = natives.table_for(Path(args.source).resolve().as_posix(), defs["tables"])
    output = args.output or str(Path(args.source).with_suffix(".stb"))
    Path(output).write_bytes(compile.compile_program(program, natives.load(args.natives)))
    return 0


def run_build(args):
    defs = loaded_definitions(args)
    found = rebuild.changes(args.source, args.iso, defs, definitions.header_texts(defs, args.sdk), natives.load(args.natives))
    for path in found["skipped"]:
        print("not on the disc, skipped: %s" % path)
    for change in found["scripts"]:
        print("changed: %s" % change["source"])
    if not found["scripts"]:
        print("no script differs from the disc")
    result = rebuild.write_disc(args.iso, args.output, found["files"])
    for item in result["placed"]:
        print("%s %s (%d bytes)" % ("moved" if item["moved"] else "in place", item["path"], item["size"]))
    if result["grown"]:
        print("the image grew by %d sectors" % result["grown"])
    wrong = rebuild.check(args.output, found["scripts"])
    for path in wrong:
        print("reads back wrong: %s" % path)
    print("%d scripts changed, %d disc files rewritten, written to %s" % (len(found["scripts"]), len(found["files"]), args.output))
    return 1 if wrong else 0


def run_verify(args):
    defs = loaded_definitions(args)
    defs, headers, _ = disc.with_header(args.extracted, defs, definitions.header_texts(defs, args.sdk))
    report = verify.verify(args.extracted, natives.load(args.natives), defs, headers)
    print(verify.format_report(report, args.limit))
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=1, ensure_ascii=False))
    return 0 if report["summary"]["ok"] else 1


def run_defs(args):
    defs = loaded_definitions(args)
    if args.defs_command == "learn":
        result = learn.learn(args.source, args.into, natives.load(args.natives), definitions.header_texts(defs, args.sdk),
                             defs["tables"])
        for table, count in sorted(result["learned"].items()):
            print("%-8s %5d functions learned" % (table, count))
        print("%d scripts with globals or docs learned" % result["scripts"])
        for failure in result["failures"]:
            print("skipped " + failure)
        return 1 if result["failures"] else 0
    if args.defs_command == "check":
        print(coverage.format_check(coverage.check(args.extracted, defs, natives.load(args.natives))))
        return 0
    if args.defs_command == "todo":
        rows, total = coverage.todo(args.extracted, defs, natives.load(args.natives),
                                    definitions.header_texts(defs, args.sdk), args.limit)
        print("%d functions still unnamed (counting language variants once); most called first:" % total)
        for row in rows:
            print("%6d  %s  %s" % (row["calls"], row["path"], row["name"]))
        return 0
    return 2


def common(command, table=False):
    command.add_argument("--natives", default=str(natives.DATA_DIR), help="directory of native table JSON files")
    command.add_argument("--sdk", default=str(sdk.SDK_DIR), help="directory of SDK headers")
    command.add_argument("--defs", action="append", help="definition set to apply; repeat to layer sets "
                         "(later ones win); the shipped PAL set when omitted; none for no definitions")
    if table:
        command.add_argument("--table", choices=[spec["table"] for spec in natives.TABLES],
                             help="native table; chosen by the definitions' path rules when omitted")


def parser():
    root = argparse.ArgumentParser(prog="cli.py", description="Dark Chronicle script tools")
    commands = root.add_subparsers(dest="command", required=True)

    command = commands.add_parser("setup", help="extract the retail disc and decompile every script with the definitions")
    command.add_argument("iso", help="the retail disc image")
    command.add_argument("output", help="directory for the decompiled source tree")
    command.add_argument("--extracted", default="extracted", help="where to extract the disc (reused when already there)")
    common(command)

    command = commands.add_parser("extract", help="write every file in the disc's DATA.DAT")
    command.add_argument("iso")
    command.add_argument("out")

    command = commands.add_parser("decompile", help="write a script, or a whole extracted disc, as c2s source")
    command.add_argument("stb", help="a script, or an extracted disc directory")
    command.add_argument("-o", "--output", help="file or directory to write; standard output for one script when omitted")
    common(command, table=True)

    command = commands.add_parser("compile", help="build a script from c2s source")
    command.add_argument("source")
    command.add_argument("-o", "--output", help="script file to write; the source path with .stb when omitted")
    common(command, table=True)

    command = commands.add_parser("build", help="compile a source tree and write a new disc image with the changed scripts")
    command.add_argument("source", help="the source tree, as setup or decompile wrote it")
    command.add_argument("iso", help="the retail disc image the source came from")
    command.add_argument("-o", "--output", required=True, help="disc image to write")
    common(command)

    command = commands.add_parser("verify", help="round-trip every script in an extracted disc")
    command.add_argument("extracted")
    command.add_argument("--json", help="also write the full report to this file")
    command.add_argument("--limit", type=int, default=20, help="failures to print")
    common(command)

    command = commands.add_parser("disassemble", help="write a script as assembly text")
    command.add_argument("stb")
    command.add_argument("-o", "--output", help="file to write; standard output when omitted")
    common(command, table=True)

    command = commands.add_parser("assemble", help="build a script from assembly text")
    command.add_argument("source")
    command.add_argument("-o", "--output", required=True, help="script file to write")
    command.add_argument("--natives", default=str(natives.DATA_DIR), help="directory of native table JSON files")

    command = commands.add_parser("defs", help="work on definition sets")
    defs_commands = command.add_subparsers(dest="defs_command", required=True)
    learn_command = defs_commands.add_parser("learn", help="record names and docs from edited source into a definition set")
    learn_command.add_argument("source", help="an edited .c2s file or source tree")
    learn_command.add_argument("into", help="definition set directory to update")
    common(learn_command)
    check_command = defs_commands.add_parser("check", help="report how much of a disc a definition set names")
    check_command.add_argument("extracted")
    common(check_command)
    todo_command = defs_commands.add_parser("todo", help="list unnamed functions, most called first")
    todo_command.add_argument("extracted")
    todo_command.add_argument("--limit", type=int, default=50, help="rows to print")
    common(todo_command)

    command = commands.add_parser("natives", help="generate native function tables from a ChronicleTwo checkout")
    command.add_argument("chronicletwo")
    command.add_argument("--out", default=str(natives.DATA_DIR), help="directory for the table JSON files")
    return root


def main():
    args = parser().parse_args()
    if args.command == "extract":
        return run_extract(args)
    if args.command == "natives":
        return run_natives(args)
    if args.command == "decompile":
        return run_decompile(args)
    if args.command == "setup":
        return run_setup(args)
    if args.command == "compile":
        return run_compile(args)
    if args.command == "disassemble":
        return run_disassemble(args)
    if args.command == "assemble":
        return run_assemble(args)
    if args.command == "defs":
        return run_defs(args)
    if args.command == "build":
        return run_build(args)
    return run_verify(args)


if __name__ == "__main__":
    sys.exit(main())
