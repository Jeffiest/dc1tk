from pathlib import Path

from c2s import compile, library, natives, parse, sdk

EXAMPLES = Path(__file__).resolve().parents[2] / "docs" / "examples"


def test_every_example_compiles_with_the_sdk():
    examples = sorted(EXAMPLES.glob("*.c"))
    assert examples
    for path in examples:
        program = parse.parse(path.read_text(encoding="utf-8"))
        expanded = library.expand(program, library.read_includes(program, path), sdk.load_texts())
        assert compile.compile_program(expanded, natives.load()), path
