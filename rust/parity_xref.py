"""Checks c2 xref, c2 maps and c2 map print what cli.py's do, across many inputs: maps; map and
xref map for every map code; xref event for every program number and name the tree defines (and
with --map for each map's own programs); xref monster for every kind by number, constant and name.
The Python runs in this process (its caches shared between inputs), c2 once per input.

  python rust/parity_xref.py decompiled extracted [--every N]

Run from the repository folder, after cargo build --release in rust/. Exit status 1 when anything
differs.
"""
import argparse
import contextlib
import copy
import io
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
C2 = REPO / "rust" / "target" / "release" / ("c2.exe" if os.name == "nt" else "c2")
sys.path.insert(0, str(REPO / "src"))

from c2s import __main__ as c2s_main, definitions, index, maps, xref  # noqa: E402


def remembered(function):
    """The tree does not change between inputs, so its scans are kept (and copied out, as callers
    may change what they get)."""
    found = {}

    def wrapper(*args):
        key = tuple(tuple(sorted(arg)) if isinstance(arg, (list, tuple, dict)) else str(arg) for arg in args)
        if key not in found:
            found[key] = function(*args)
        return copy.deepcopy(found[key])
    return wrapper


index.calls = remembered(index.calls)
index.outlines = remembered(index.outlines)


def python(argv):
    out, err = io.StringIO(), io.StringIO()
    sys.argv = ["cli.py"] + argv
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            code = c2s_main.main()
        except SystemExit as stop:
            if isinstance(stop.code, str):
                err.write(stop.code + "\n")
                code = 1
            else:
                code = stop.code
    return code or 0, out.getvalue(), err.getvalue()


def rust(argv):
    result = subprocess.run([str(C2)] + argv, capture_output=True, encoding="utf-8")
    return result.returncode, result.stdout.replace("\r\n", "\n"), result.stderr.replace("\r\n", "\n")


def inputs(tree, extracted, every):
    defs = definitions.load([definitions.DEFAULT_SET])
    consts = index.constants(index.tree_headers(tree, defs))
    found = maps.catalog(tree, extracted, defs, consts)
    codes = sorted(found)
    yield ["maps"]
    for code in codes + ["zz99"]:
        yield ["map", code]
        yield ["xref", "map", code]
    outline = index.outlines(tree)
    numbers, names = set(), set()
    for entry in outline.values():
        for program in entry["programs"]:
            number = index.value(program["number"], consts)
            if isinstance(number, int):
                numbers.add(number)
                if not program["number"].lstrip("-").isdigit():
                    names.add(program["number"])
    for number in sorted(numbers)[::every]:
        yield ["xref", "event", str(number)]
    for name in sorted(names)[::every]:
        yield ["xref", "event", name]
    for code in codes[::every]:
        for number in sorted(found[code]["programs"])[:6]:
            yield ["xref", "event", str(number), "--map", code]
    yield ["xref", "event", "no_such_program"]
    yield ["xref", "event", "1", "--map", "zz99"]
    rows = xref.monster_rows(extracted, defs)
    for kind, row in list(rows.items())[::every]:
        yield ["xref", "monster", str(kind)]
        if row["name"]:
            yield ["xref", "monster", row["name"].upper()]
    for name in sorted(name for name in consts if name.startswith("MONSTER_"))[::every]:
        yield ["xref", "monster", name]
    yield ["xref", "monster", "no such monster"]
    yield ["xref", "monster", "-1"]


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tree")
    parser.add_argument("extracted")
    parser.add_argument("--every", type=int, default=1, help="check every Nth number, name and kind")
    args = parser.parse_args()
    tree, extracted = str(Path(args.tree).resolve()), str(Path(args.extracted).resolve())
    commands = [argv[:1] + [tree] + argv[1:] + ["--extracted", extracted] for argv in inputs(tree, extracted, args.every)]
    start = time.time()
    expected = [python(argv) for argv in commands]
    python_time = time.time() - start
    start = time.time()
    with ThreadPoolExecutor(max_workers=os.cpu_count()) as pool:
        got = list(pool.map(rust, commands))
    rust_time = time.time() - start
    wrong = 0
    for argv, want, have in zip(commands, expected, got):
        if want != have:
            wrong += 1
            if wrong <= 10:
                print("different: %s" % " ".join(argv[:1] + argv[2:-2]))
                print("  python: %r" % (want,))
                print("  c2:     %r" % (have,))
    print("%d inputs compared, %d different (Python %.1f s in one process, c2 %.1f s in %d processes at once)"
          % (len(commands), wrong, python_time, rust_time, len(commands)))
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
