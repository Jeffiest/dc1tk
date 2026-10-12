import os
from pathlib import Path

from c2s import asm, compile, decompile, definitions, library, names, natives, notes, parse, sdk, source, stb

EXAMPLES = Path(__file__).resolve().parents[2] / "docs" / "examples"


def example_bytes(directory, defs):
    path = directory / (directory.name + ".c2s")
    program = parse.parse(path.read_text(encoding="utf-8"))
    headers = definitions.header_texts(defs, sdk.SDK_DIR)
    return compile.compile_program(library.expand(program, library.read_includes(program, path), headers), natives.load())


def decompiled(data, table, defs, headers, path):
    tables = natives.load()
    program = decompile.decompile(stb.read(data), table, tables)
    found = sdk.header_context(table, headers, tables, defs["include"])
    program_names = found["program_names"] if found is not None else {}
    entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
    named = names.apply(program, entries, sdk.reserved(found), program_names)
    scripted = names.with_script(named, definitions.script_entry(defs, path))
    return source.text(notes.annotate(sdk.present(scripted, found), defs["strings"]))


def stages(directory):
    defs = definitions.load([directory / "definitions"])
    data = example_bytes(directory, defs)
    table = "event"
    script = directory.name + ".stb"
    return {
        directory.name + ".asm": asm.disassemble(stb.read(data), table, natives.load()),
        directory.name + ".bare.c2s": decompiled(data, table, definitions.empty(), {}, script),
        directory.name + ".sdk.c2s": decompiled(data, table, definitions.empty(), sdk.load_texts(), script),
        directory.name + ".c2s": decompiled(data, table, defs, definitions.header_texts(defs, sdk.SDK_DIR), script),
    }, data


def test_examples_match_the_decompiler():
    for directory in sorted(path for path in EXAMPLES.iterdir() if path.is_dir()):
        outputs, data = stages(directory)
        for name, text in outputs.items():
            path = directory / name
            if os.environ.get("C2S_REGENERATE") and name != directory.name + ".c2s":
                path.write_text(text, encoding="utf-8")
            assert path.read_text(encoding="utf-8") == text, "%s is out of date" % path


def test_every_example_stage_compiles_to_the_same_bytes():
    for directory in sorted(path for path in EXAMPLES.iterdir() if path.is_dir()):
        outputs, data = stages(directory)
        assert asm.assemble(outputs[directory.name + ".asm"], natives.load()) == data
        bare = parse.parse(outputs[directory.name + ".bare.c2s"])
        assert compile.compile_program(bare, natives.load()) == data
        with_sdk = parse.parse(outputs[directory.name + ".sdk.c2s"])
        assert compile.compile_program(library.expand(with_sdk, {}, sdk.load_texts()), natives.load()) == data
