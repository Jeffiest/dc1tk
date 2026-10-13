import argparse
import json
import sys
from pathlib import Path

from c2s import (asm, assets, compile, coverage, decompile, definitions, disc, discfiles, extract, flags, index, learn, library, messages, names,
                 maps, mods, natives, notes, parse, patches, pathnames, play, rebuild, sdk, slots, source, startat, stb, tree,
                 verify, xref)


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


# Whether an extracted disc is the one the definition sets were made for.
def disc_match_text(root, defs, differences=None):
    if not defs["disc"]:
        return "the definition sets record no disc to compare with"
    differences = differences or definitions.disc_differences(root, defs)
    if not any(differences.values()):
        return "the disc is the one the definition sets were made for (%d scripts match)" % len(defs["disc"])
    return ("the disc is not the one the definition sets were made for: %d scripts differ, %d are new, %d are "
            "missing (first: %s); names and docs may not fit" % (
                len(differences["differ"]), len(differences["unknown"]), len(differences["missing"]),
                (differences["differ"] + differences["unknown"] + differences["missing"])[0]))


def run_decompile(args):
    tables = natives.load(args.natives)
    defs = loaded_definitions(args)
    headers = definitions.header_texts(defs, args.sdk)
    if Path(args.stb).is_dir():
        return run_decompile_disc(args, tables, defs, headers)
    table = chosen_table(args, args.stb, defs)
    program = decompile.decompile(stb.read(Path(args.stb).read_bytes()), table, tables)
    found = sdk.with_sound_sets(sdk.header_context(table, headers, tables, defs["include"]), defs)
    program_names = found["program_names"] if found is not None else {}
    entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
    named = names.apply(program, entries, sdk.reserved(found), program_names)
    named = with_messages(named, found, defs, args.stb)
    entry = definitions.script_entry(defs, args.stb)
    scripted = names.with_script(named, entry)
    params = messages.kind_params(named["functions"], found) if found is not None and "asm" not in named else {}
    found = sdk.numbered(found, sdk.numbering_of(entry))
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
    # a folder of the disc is compared with that folder of the set's disc
    differences = definitions.disc_differences(args.stb, defs, "" if prefix == "." else prefix + "/")
    if differences["differ"] or differences["unknown"]:
        print("warning: " + disc_match_text(args.stb, defs, differences))
    paths = [path if prefix == "." else prefix + "/" + path for path in verify.find_scripts(args.stb)]
    headers_used = headers
    defs, headers, generated = disc.with_header(str(root), defs, headers)
    model = library.model_for(str(root), paths, tables, defs, headers)
    generated = slots.with_header(generated, model.get("folder_slots"))
    generated = assets.with_header(generated, model.get("asset_names"))
    naming = pathnames.lookups(str(root), defs)
    renamed = {library_entry["path"]: pathnames.named_library(library_entry["path"], naming)
               for library_entry in model["libraries"].values()}
    model = pathnames.with_named_libraries(model, naming)
    library_paths = [library_entry["path"] for library_entry in model["libraries"].values()]
    for path in paths:
        plain = Path(path).with_suffix(source.EXTENSION).as_posix()
        renamed[plain] = pathnames.named_script(plain, naming)
    written = library_paths + [renamed[Path(path).with_suffix(source.EXTENSION).as_posix()] for path in paths] + [source.EDITOR_FLAGS]
    if generated is not None:
        written.append("include/" + disc.HEADER)
    blocked = tree.overwrites(out, written) if out.is_dir() else []
    if blocked and not args.force:
        raise SystemExit("%s holds %d edited file%s this would overwrite, such as:\n  %s\n"
                         "Save them first (cli.py mod save %s NAME), write somewhere else, or pass --force."
                         % (out, len(blocked), "" if len(blocked) == 1 else "s", "\n  ".join(blocked[:10]), out))
    if generated is not None:
        (out / "include").mkdir(parents=True, exist_ok=True)
        (out / "include" / disc.HEADER).write_text(generated, encoding="utf-8")
    for path in paths:
        table = natives.table_for(path, defs["tables"])
        program = decompile.decompile(stb.read((root / path).read_bytes()), table, tables)
        target = out / renamed[Path(path).with_suffix(source.EXTENSION).as_posix()]
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(source.text(library.script_program(program, path, model)), encoding="utf-8")
    # libraries last: the scripts that include one tell its sound pass what they pass it
    for path, text in library.library_texts(model).items():
        (out / path).parent.mkdir(parents=True, exist_ok=True)
        (out / path).write_text(text, encoding="utf-8")
    (out / source.EDITOR_FLAGS).write_text(source.editor_flags(args.sdk, defs["sets"]), encoding="utf-8")
    removed = []
    if prefix == ".":
        # a whole-disc decompile replaces the tree: files the last one wrote and this one does not
        # (a library the definitions moved) go, unless someone edited them
        old = tree.stale(out, written)
        tree.remove_files(out, old["unedited"])
        removed = old["unedited"]
        for path in old["edited"]:
            print("kept %s: the decompiler no longer writes it, but it was edited" % path)
        if removed:
            print("removed %d file%s the decompiler no longer writes" % (len(removed), "" if len(removed) == 1 else "s"))
    tree.record(out, written, tree.headers_hash(headers_used), removed)
    pathnames.record(out, renamed)
    print("%d scripts and %d library files written to %s" % (len(paths), len(library_paths), out))
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
    program = library.expand(program, library.read_includes(program, args.source, None, args.sdk), headers)
    if "asm" not in program and program["natives"] is None:
        program["natives"] = natives.table_for(Path(args.source).resolve().as_posix(), defs["tables"])
    output = args.output or str(Path(args.source).with_suffix(".stb"))
    Path(output).write_bytes(compile.compile_program(program, natives.load(args.natives)))
    return 0


def run_build(args):
    defs = loaded_definitions(args)
    mod_dirs = [mods.folder(args.mods, name) for name in args.mod]
    overrides, clashes = mods.overlay(mod_dirs, args.source)
    if clashes:
        raise SystemExit(mods.clash_text(clashes))
    disc_files, disc_clashes = discfiles.collect(mod_dirs)
    if disc_clashes:
        raise SystemExit(discfiles.clash_text(disc_clashes))
    consts = index.constants(index.tree_headers(args.source, defs, args.sdk))
    plan = mods.patch_plan(mod_dirs, args.source, consts)
    for mod_dir in mod_dirs:
        mine = [patch for patch in plan["patches"] if patch["mod"] == mod_dir.name]
        print("mod %s: %d patches, %d files, %d game files" % (mod_dir.name, len(mine), len(mods.contents(mod_dir, args.source)),
                                                             len(discfiles.mod_files(mod_dir))))
        for patch in mine:
            print("  patch " + patches.summary(patch))
        for name, number in sorted(plan["numbers"].get(mod_dir.name, {}).items()):
            print("  %s = %d" % (name, number))
        for path in mods.stale(mod_dir, args.source):
            print("warning: %s changed in the decompiler's output since %s was made; compare them" % (path, mod_dir.name))
    start_at = args.start_at
    if start_at is None and not args.no_start_at:
        wanted = {mods.entries(mod_dir).get("start_at") for mod_dir in mod_dirs} - {None, ""}
        if len(wanted) > 1:
            raise SystemExit("the chosen mods start at different scenes (%s); pick one with --start-at, or --no-start-at"
                             % ", ".join(sorted(wanted)))
        if wanted:
            start_at = wanted.pop()
            print("start at %s, from the chosen mod (--no-start-at for a normal New Game)" % start_at)
    if start_at:
        target = startat.parse_target(start_at)
        given = {name: number for found in plan["numbers"].values() for name, number in found.items()}
        if target["program"] in given:
            # a mod's NEW_PROGRAM name
            target["program"] = str(given[target["program"]])
        for warning in startat.warnings(args.source, consts, target):
            print("warning: " + warning)
        patched = startat.overrides(args.source, target, overrides)
        overrides = dict(overrides, **patched)
        print("start at: a new game goes to map %s at program %s (patched %s; flag %d marks it done)"
              % (target["map"], target["program"], ", ".join(sorted(patched)), startat.START_FLAG))
    status = tree.status(args.source)
    if status is not None:
        if mod_dirs and (status["edited"] or status["added"]):
            print("note: %s itself has %d edited or added files, which are built in under the mods"
                  % (args.source, len(status["edited"]) + len(status["added"])))
        for note in tree.library_notes(args.source, status):
            print("note: " + note)
    found = rebuild.changes(args.source, args.iso, defs, definitions.header_texts(defs, args.sdk), natives.load(args.natives),
                            overrides, args.full, args.sdk, plan["scripts"])
    if found["all_because"]:
        print("compiled all %d scripts: %s" % (found["compiled"], found["all_because"]))
    else:
        print("compiled %d scripts; the rest are unchanged since decompiling" % found["compiled"])
    for path in found["skipped"]:
        print("not on the disc, skipped: %s" % path)
    for change in found["scripts"]:
        print("changed: %s" % change["source"])
    if not found["scripts"]:
        print("no script differs from the disc")
    try:
        files, replaced = discfiles.replaced(args.iso, disc_files, found["files"], {change["top"] for change in found["scripts"]})
    except ValueError as error:
        raise SystemExit(str(error))
    for item in replaced:
        print("replaced: %s (mod %s, %d bytes, was %d)" % (item["path"], item["mod"], item["new"], item["old"]))
        if item["new"] > item["old"]:
            # models, textures and sounds load into fixed memory the build cannot see
            print("warning: %s grew by %d bytes; if the game runs short of memory where it loads, keep it nearer "
                  "the original's size" % (item["path"], item["new"] - item["old"]))
    result = rebuild.write_disc(args.iso, args.output, files)
    for item in result["placed"]:
        print("%s %s (%d bytes)" % ("moved" if item["moved"] else "in place", item["path"], item["size"]))
    if result["grown"]:
        print("the image grew by %d sectors" % result["grown"])
    wrong = rebuild.check(args.output, found["scripts"]) + discfiles.check(args.output, replaced)
    for path in wrong:
        print("reads back wrong: %s" % path)
    print("%d scripts changed, %d game files replaced, %d disc files rewritten, written to %s"
          % (len(found["scripts"]), len(replaced), len(files), args.output))
    if wrong:
        return 1
    if args.play:
        print("started PCSX2: %s" % play.launch(args.output, args.pcsx2))
    return 0


def tree_constants(args, defs):
    return index.constants(index.tree_headers(args.source, defs, args.sdk))


def run_flags(args):
    defs = loaded_definitions(args)
    consts = tree_constants(args, defs)
    found = flags.scan(args.source, consts)
    if args.flag is not None:
        print(flags.who(found, args.flag, consts))
    elif args.used:
        print(flags.used_list(found, consts))
    else:
        print(flags.summary(found))
    return 0


def run_xref(args):
    defs = loaded_definitions(args)
    consts = tree_constants(args, defs)
    if args.kind == "map":
        print(xref.map_report(args.source, consts, args.what))
        return 0
    rows = xref.monster_rows(args.extracted, defs)
    if args.kind == "monster":
        print(xref.monster_report(args.source, consts, rows, args.what))
        return 0
    number = index.value(args.what, consts)
    if not isinstance(number, int):
        raise SystemExit("%s is not a program number or name" % args.what)
    print(xref.event_report(args.source, consts, number, args.map, rows))
    return 0


def run_maps(args):
    defs = loaded_definitions(args)
    consts = tree_constants(args, defs)
    found = maps.catalog(args.source, args.extracted, defs, consts)
    if args.command == "maps":
        print(maps.summary(found))
    else:
        programs = index.enum_values(index.tree_headers(args.source, defs, args.sdk), "program")
        print(maps.detail(args.name, found, args.source, args.extracted, consts, programs))
    return 0


def run_play(args):
    found = play.launch(args.image, args.pcsx2, not args.full_boot)
    print("started %s with %s" % (args.image, found))
    return 0


def run_changes(args):
    print(tree.changes_text(args.source))
    return 0


def run_mod(args):
    if args.mod_command == "test":
        defs = loaded_definitions(args)
        results = mods.run_tests(mods.folder(args.mods, args.name), args.source, defs, args.sdk)
        for name, passed, printed, error in results:
            print("%-5s %s" % ("ok" if passed else "FAIL", name))
            if not passed:
                for line in printed:
                    print("      printed: " + line)
                if error:
                    print("      error: " + error)
        failed = sum(1 for result in results if not result[1])
        print("%d passed, %d failed" % (len(results) - failed, failed))
        return 1 if failed else 0
    if args.mod_command == "list":
        print(mods.listing(args.mods, args.source))
        return 0
    if args.mod_command == "apply":
        result = mods.apply([mods.folder(args.mods, name) for name in args.names], args.source, args.force)
        for warning in result["warnings"]:
            print("warning: " + warning)
        print("%d files applied to %s" % (len(result["applied"]), args.source))
        return 0
    if args.mod_command == "check":
        defs = loaded_definitions(args)
        result = mods.check([mods.folder(args.mods, name) for name in args.names], args.source, defs,
                            natives.load(args.natives), args.sdk)
        for name, found in sorted(result["numbers"].items()):
            for constant, number in sorted(found.items()):
                print("%s: %s = %d" % (name, constant, number))
        for path, error in result["errors"]:
            print("error: %s: %s" % (path, error))
        print("%d scripts compiled, %d failed" % (result["compiled"], len(result["errors"])))
        return 1 if result["errors"] else 0
    mod_dir = mods.folder(args.mods, args.name)
    if args.mod_command == "asset":
        try:
            written = discfiles.copy_out(args.iso, mod_dir, args.paths, args.form)
        except ValueError as error:
            raise SystemExit(str(error))
        for path in written:
            print("%s: replace it there, and build puts it on the disc" % (mod_dir / discfiles.DISC / path))
        return 0
    if args.mod_command == "override":
        kinds = [(kind, getattr(args, kind)) for kind in parse.TARGET_KINDS if getattr(args, kind)]
        if len(kinds) != 1:
            raise SystemExit("give one of --map, --script, --monster and --library: the scripts to patch")
        result = mods.override(args.source, mod_dir, kinds[0][0], kinds[0][1], args.functions)
        print("%s: %s" % (result["path"], ", ".join(result["added"]) if result["added"] else "already overrides them"))
        print("patches %d script%s; edit the functions there, and call original() to run the script's own"
              % (len(result["scripts"]), "" if len(result["scripts"]) == 1 else "s"))
        mods.write_patch_flags(mod_dir, args.source, args.sdk, loaded_definitions(args)["sets"])
        return 0
    if args.mod_command == "new":
        copied = mods.new(args.source, mod_dir, args.files)
        print("%d files copied into %s; edit them there" % (len(copied), mod_dir / mods.FILES))
    else:
        result = mods.save(args.source, mod_dir)
        print("%d files saved to %s" % (len(result["saved"]), mod_dir))
        for path in result["removed"]:
            print("note: %s was removed from the tree; a mod cannot remove files" % path)
    defs = loaded_definitions(args)
    mods.write_editor_flags(mod_dir, args.source, args.sdk, defs["sets"])
    return 0


def run_fork(args):
    result = tree.fork(args.source, args.library, args.copy, args.scripts)
    print("%s now includes %s" % (", ".join(args.scripts), args.copy))
    print("%d scripts still include %s" % (len(result["still_using_original"]), args.library))
    return 0


def run_verify(args):
    defs = loaded_definitions(args)
    defs, headers, _ = disc.with_header(args.extracted, defs, definitions.header_texts(defs, args.sdk))
    report = verify.verify(args.extracted, natives.load(args.natives), defs, headers)
    print(verify.format_report(report, args.limit))
    if args.json:
        Path(args.json).write_text(json.dumps(report, indent=1, ensure_ascii=False), encoding="utf-8")
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
        print(disc_match_text(args.extracted, defs))
        return 0
    if args.defs_command == "disc":
        print("%d script hashes written to %s" % (definitions.write_disc(args.extracted, args.into),
                                                  Path(args.into) / definitions.DISC))
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
    command.add_argument("--force", action="store_true", help="overwrite files edited since the last decompile")
    common(command)

    command = commands.add_parser("extract", help="write every file in the disc's DATA.DAT")
    command.add_argument("iso")
    command.add_argument("out")

    command = commands.add_parser("decompile", help="write a script, or a whole extracted disc, as C source")
    command.add_argument("stb", help="a script, or an extracted disc directory")
    command.add_argument("-o", "--output", help="file or directory to write; standard output for one script when omitted")
    command.add_argument("--force", action="store_true", help="overwrite files edited since the last decompile")
    common(command, table=True)

    command = commands.add_parser("compile", help="build a script from C source")
    command.add_argument("source")
    command.add_argument("-o", "--output", help="script file to write; the source path with .stb when omitted")
    common(command, table=True)

    command = commands.add_parser("build", help="compile a source tree and write a new disc image with the changed scripts")
    command.add_argument("source", help="the source tree, as setup or decompile wrote it")
    command.add_argument("iso", help="the retail disc image the source came from")
    command.add_argument("-o", "--output", required=True, help="disc image to write")
    command.add_argument("--start-at", metavar="MAP:PROGRAM",
                         help="test image only: a new game goes straight to this map and program, as in s51:story_50")
    command.add_argument("--mod", action="append", default=[], metavar="NAME",
                         help="layer this mod over the tree; repeat for several, which may not change the same file")
    command.add_argument("--mods", default="mods", help="the folder that holds the mods")
    command.add_argument("--no-start-at", action="store_true", help="ignore a chosen mod's start_at")
    command.add_argument("--play", action="store_true", help="boot the image in PCSX2 when it is built")
    command.add_argument("--pcsx2", help="PCSX2's executable, for --play")
    command.add_argument("--full", action="store_true",
                         help="compile every script, not only those edited, modded or including an edited library")
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
    learn_command.add_argument("source", help="an edited .c file or source tree")
    learn_command.add_argument("into", help="definition set directory to update")
    common(learn_command)
    check_command = defs_commands.add_parser("check", help="report how much of a disc a definition set names")
    check_command.add_argument("extracted")
    common(check_command)
    todo_command = defs_commands.add_parser("todo", help="list unnamed functions, most called first")
    todo_command.add_argument("extracted")
    todo_command.add_argument("--limit", type=int, default=50, help="rows to print")
    common(todo_command)
    disc_command = defs_commands.add_parser("disc", help="record which disc a definition set is for: its scripts' hashes")
    disc_command.add_argument("extracted")
    disc_command.add_argument("into", help="definition set directory to write disc.json into")
    common(disc_command)

    command = commands.add_parser("flags", help="save flags the scripts and engine use, and which are free")
    command.add_argument("source", help="a decompiled source tree")
    command.add_argument("--used", action="store_true", help="list every used flag with its name and users")
    command.add_argument("--flag", type=int, help="show every use of this flag")
    common(command)

    command = commands.add_parser("xref", help="what starts an event, what enters a map, where a monster is used")
    command.add_argument("source", help="a decompiled source tree")
    command.add_argument("kind", choices=["event", "map", "monster"])
    command.add_argument("what", help="a program number or name, a map name (s51), or a monster number, constant or name")
    command.add_argument("--map", help="for event: only this map's programs and entries")
    command.add_argument("--extracted", default="extracted", help="the extracted disc, for monster models and AI scripts")
    common(command)

    command = commands.add_parser("maps", help="every map: how the game uses it and whether the player plays there")
    command.add_argument("source", help="a decompiled source tree")
    command.add_argument("--extracted", default="extracted", help="the extracted disc, for map names")
    common(command)

    command = commands.add_parser("map", help="one map: programs and free numbers, trigger points, player positions, entries")
    command.add_argument("source", help="a decompiled source tree")
    command.add_argument("name", help="the map's code, as in s17")
    command.add_argument("--extracted", default="extracted", help="the extracted disc, for names and .map files")
    common(command)

    command = commands.add_parser("play", help="boot a disc image in PCSX2")
    command.add_argument("image", help="the disc image")
    command.add_argument("--pcsx2", help="PCSX2's executable; else $PCSX2, the PATH or the usual install folder")
    command.add_argument("--full-boot", action="store_true", help="show the PS2 boot screens instead of skipping them")

    command = commands.add_parser("changes", help="list files edited, added or removed since the tree was decompiled")
    command.add_argument("source", help="a decompiled source tree")

    command = commands.add_parser("mod", help="make, list and apply mods: folders of changed files layered over a tree")
    mod_commands = command.add_subparsers(dest="mod_command", required=True)
    new_command = mod_commands.add_parser("new", help="copy tree files into a mod, to edit them there")
    new_command.add_argument("source", help="the decompiled source tree")
    new_command.add_argument("name", help="the mod's name, a folder in the mods folder")
    new_command.add_argument("files", nargs="+", help="tree files to copy into the mod, relative to the tree")
    save_command = mod_commands.add_parser("save", help="copy every file edited or added in the tree into a mod")
    save_command.add_argument("source", help="the edited source tree")
    save_command.add_argument("name", help="the mod's name, a folder in the mods folder")
    list_command = mod_commands.add_parser("list", help="list the mods, their files, and mods that change the same file")
    list_command.add_argument("source", help="the decompiled source tree")
    test_command = mod_commands.add_parser("test", help="run the test_ functions of a mod's tests.c without the game")
    test_command.add_argument("source", help="the decompiled source tree")
    test_command.add_argument("name", help="the mod's name")
    apply_command = mod_commands.add_parser("apply", help="write mods' files into a source tree")
    apply_command.add_argument("source", help="the source tree to apply them to")
    apply_command.add_argument("names", nargs="+", help="mods to apply, in order")
    apply_command.add_argument("--force", action="store_true", help="replace files edited in the tree too")
    override_command = mod_commands.add_parser(
        "override", help="start a patch: copy functions of a script into the mod, marked to replace the script's")
    override_command.add_argument("source", help="the decompiled source tree")
    override_command.add_argument("name", help="the mod's name")
    override_command.add_argument("functions", nargs="+", help="functions to override, as the script names them")
    override_command.add_argument("--map", help="patch every language of a map's script (s51)")
    override_command.add_argument("--script", help="patch scripts by path without readable names (dungeon/monster/e33a.c)")
    override_command.add_argument("--monster", help="patch a monster kind's AI script (golem)")
    override_command.add_argument("--library", help="patch every script that includes a library (lib/...)")
    asset_command = mod_commands.add_parser(
        "asset", help="copy game files off the disc (models, textures, sounds) into a mod, to replace them there")
    asset_command.add_argument("iso", help="the disc image")
    asset_command.add_argument("name", help="the mod's name")
    asset_command.add_argument("paths", nargs="+",
                               help="files as extract lays them out: snd2/sp/sp_000.snd, or a pack member "
                                    "as chara/c01_base.chr.d/c01b.img")
    asset_command.add_argument("--as", dest="form", help="export in a common format to edit there: png for textures")
    check_command = mod_commands.add_parser("check", help="compile what mods change, without a disc, and show their numbers")
    check_command.add_argument("source", help="the decompiled source tree")
    check_command.add_argument("names", nargs="+", help="mods to check together, in build order")
    for mod_command in (new_command, save_command, list_command, apply_command, test_command, override_command,
                        check_command, asset_command):
        mod_command.add_argument("--mods", default="mods", help="the folder that holds the mods")
    common(new_command)
    common(save_command)
    common(test_command)
    common(override_command)
    common(check_command)

    command = commands.add_parser("fork", help="give scripts their own copy of a library, so edits to it reach only them")
    command.add_argument("source", help="a decompiled source tree")
    command.add_argument("library", help="the library, as scripts include it (lib/...)")
    command.add_argument("copy", help="path for the copy (lib/...)")
    command.add_argument("scripts", nargs="+", help="scripts to switch to the copy, relative to the tree")

    command = commands.add_parser("natives", help="generate native function tables from a ChronicleTwo checkout")
    command.add_argument("chronicletwo")
    command.add_argument("--out", default=str(natives.DATA_DIR), help="directory for the table JSON files")
    return root


def main():
    # Scripts and reports carry Japanese text; without this a Windows console's code page fails on it.
    for stream in (sys.stdout, sys.stderr):
        if hasattr(stream, "reconfigure"):
            stream.reconfigure(encoding="utf-8")
    args = parser().parse_args()
    try:
        return run(args)
    except ValueError as error:
        # Compile and parse errors name the file and line; a traceback would only hide that.
        sys.stderr.write("error: %s\n" % error)
        return 1


def run(args):
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
    if args.command == "flags":
        return run_flags(args)
    if args.command == "xref":
        return run_xref(args)
    if args.command == "changes":
        return run_changes(args)
    if args.command == "play":
        return run_play(args)
    if args.command in ("maps", "map"):
        return run_maps(args)
    if args.command == "mod":
        return run_mod(args)
    if args.command == "fork":
        return run_fork(args)
    return run_verify(args)


if __name__ == "__main__":
    sys.exit(main())
