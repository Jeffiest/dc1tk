"""Checks c2 defs (learn, check, todo, disc) against cli.py defs: the same output, exit codes and
written files. Run from the repository folder:

    python rust/parity_defs.py EXTRACTED DECOMPILED SCRATCH [--quick]

SCRATCH is a folder for copies (made empty first): definition sets are learned into copies of
definitions/pal, never into it, and the decompiled tree is learned from as it is and from an
edited copy. --quick skips learning from the whole trees.
"""
import argparse
import filecmp
import random
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
C2 = REPO / "rust" / "target" / "release" / ("c2.exe" if sys.platform == "win32" else "c2")
failures = []


def run(command):
    start = time.perf_counter()
    done = subprocess.run(command, capture_output=True, cwd=REPO)
    # standard output compared as text, as parity.py does (Python writes CRLF to a pipe on Windows)
    return done.returncode, done.stdout.replace(b"\r\n", b"\n"), done.stderr, time.perf_counter() - start


def compare(name, args, before=None):
    """Runs cli.py and c2 with the same arguments; before(tool) prepares each run and returns the
    folder whose files are compared after it, if any."""
    results = {}
    for tool, command in (("py", [sys.executable, "cli.py"]), ("rs", [str(C2)])):
        folder = before(tool) if before else None
        results[tool] = run(command + args) + (folder,)
    (py_code, py_out, py_err, py_time, py_dir), (rs_code, rs_out, rs_err, rs_time, rs_dir) = results["py"], results["rs"]
    problems = []
    if py_code != rs_code:
        problems.append("exit %d vs %d" % (py_code, rs_code))
    if py_out != rs_out:
        problems.append("output differs:\n--- py\n%s\n--- rs\n%s" % (py_out.decode(errors="replace")[-3000:],
                                                                    rs_out.decode(errors="replace")[-3000:]))
    if py_code and py_err.strip().splitlines()[-1:] and not rs_err:
        problems.append("py failed: " + py_err.decode(errors="replace")[-2000:])
    if py_dir is not None:
        problems += tree_differences(py_dir, rs_dir)
    lines = py_out.decode(errors="replace").splitlines()
    print("%-40s %s  py %.1fs  rs %.1fs  (%d lines)" % (name, "ok" if not problems else "DIFFERS", py_time, rs_time,
                                                         len(lines)))
    for problem in problems:
        print("    " + problem.replace("\n", "\n    "))
    if problems:
        failures.append(name)
    return py_out


def tree_differences(left, right):
    left, right = Path(left), Path(right)
    files = lambda base: sorted(path.relative_to(base).as_posix() for path in base.rglob("*") if path.is_file())
    a, b = files(left), files(right)
    out = ["only in py: " + path for path in sorted(set(a) - set(b))] + ["only in rs: " + path for path in sorted(set(b) - set(a))]
    for path in sorted(set(a) & set(b)):
        if not filecmp.cmp(left / path, right / path, shallow=False):
            out.append("differs: " + path)
    return out[:40]


def fresh_set(scratch, tool, label):
    target = scratch / ("%s-%s" % (label, tool))
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(REPO / "definitions" / "pal", target)
    return target


DEFINITION = re.compile(r"^(?:void|int|float) (\w+)\(", re.M)
LOCAL = re.compile(r"^    (?:int|float|string) (\w+)(?:\[\d+\])?;$", re.M)
GLOBAL = re.compile(r"^(?:int|float|string) (\w+)(?:\[\d+\])?;$", re.M)
STATEMENT = re.compile(r"^    (?!int |float |string )\w[^\n]*;$", re.M)


def rename(text, old, new):
    return re.sub(r"\b%s\b" % re.escape(old), new, text)


def edit(text, rng, number):
    """Some of the edits a modder makes: names, docs, locals, globals, notes."""
    functions = DEFINITION.findall(text)
    if functions:
        name = rng.choice(functions)
        choice = rng.randrange(4)
        if choice == 0:
            text = rename(text, name, "%s_edited%d" % (name, number))
        elif choice == 1:
            text = rename(text, name, "func_%d" % (900 + number))  # a default name: not learned
        text = re.sub(r"^((?:void|int|float) %s\()" % re.escape(DEFINITION.findall(text)[functions.index(name)]),
                      "// Edited doc %d - with a hyphen-\n// joined line.\n\\1" % number, text, count=1, flags=re.M)
    locals_ = LOCAL.findall(text)
    if locals_:
        text = rename(text, rng.choice(locals_), "edited_local_%d" % number)
    globals_ = GLOBAL.findall(text)
    if globals_ and rng.random() < 0.5:
        text = rename(text, rng.choice(globals_), "edited_global_%d" % number)
    statements = list(STATEMENT.finditer(text))
    if statements:
        at = rng.choice(statements).start()
        text = text[:at] + "    // note: edited note %d.\n" % number + text[at:]
    if text.startswith("//") and rng.random() < 0.5:
        text = "// Edited file doc %d.\n" % number + text
    return text


def edited_tree(decompiled, scratch):
    tree = scratch / "edited-tree"
    if tree.exists():
        shutil.rmtree(tree)
    shutil.copytree(decompiled, tree)
    rng = random.Random(2)
    scripts = sorted(path for path in tree.rglob("*.c") if path.relative_to(tree).parts[0] != "lib")
    libraries = sorted(path for path in (tree / "lib").rglob("*.c"))
    chosen = rng.sample(scripts, 120) + rng.sample(libraries, 15)
    for number, path in enumerate(chosen):
        text = path.read_text(encoding="utf-8")
        if path.is_relative_to(tree / "lib"):
            # a library's doc and a function's name and doc; every script that includes it agrees
            functions = DEFINITION.findall(text)
            if functions and number % 2:
                text = rename(text, functions[0], "lib_edited_%d" % number)
            text = "// Edited library doc %d.\n" % number + text
        else:
            text = edit(text, rng, number)
        path.write_text(text, encoding="utf-8")
    broken = scripts[7]
    broken.write_text(broken.read_text(encoding="utf-8").replace("{", "{ @", 1), encoding="utf-8")
    return tree, chosen[:3] + [broken]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("extracted")
    parser.add_argument("decompiled")
    parser.add_argument("scratch")
    parser.add_argument("--quick", action="store_true")
    args = parser.parse_args()
    extracted, decompiled, scratch = Path(args.extracted).resolve(), Path(args.decompiled).resolve(), Path(args.scratch).resolve()
    if scratch.exists():
        shutil.rmtree(scratch)
    scratch.mkdir(parents=True)

    compare("defs check", ["defs", "check", str(extracted)])
    compare("defs check --defs none", ["defs", "check", str(extracted), "--defs", "none"])
    for limit in ("50", "7", "0", "-3", "100000"):
        compare("defs todo --limit " + limit, ["defs", "todo", str(extracted), "--limit", limit])
    compare("defs todo --defs none", ["defs", "todo", str(extracted), "--limit", "20", "--defs", "none"])

    disc_dir = scratch / "disc"

    def disc_before(tool):
        if disc_dir.exists():
            shutil.move(str(disc_dir), str(scratch / "disc-py"))
        disc_dir.mkdir()
    compare("defs disc", ["defs", "disc", str(extracted), str(disc_dir)], disc_before)
    differences = tree_differences(scratch / "disc-py", disc_dir)
    print("defs disc files: %s" % ("same" if not differences else differences))
    if differences:
        failures.append("defs disc files")
    folder = extracted / "map" / "s" / "s51"
    if folder.is_dir():
        compare("defs disc (one folder)", ["defs", "disc", str(folder), str(disc_dir)], None)

    tree, singles = edited_tree(decompiled, scratch)
    for number, path in enumerate(singles):
        compare_learn("defs learn file %d (%s)" % (number, path.name), path,
                      lambda tool, number=number: fresh_set(scratch, tool, "file%d" % number), scratch, "file%d" % number)
    outside = scratch / "outside.c"
    outside.write_text(singles[0].read_text(encoding="utf-8"), encoding="utf-8")
    compare_learn("defs learn file outside a tree", outside, lambda tool: fresh_set(scratch, tool, "outside"),
                  scratch, "outside")
    if not args.quick:
        compare_learn("defs learn edited tree", tree, lambda tool: fresh_set(scratch, tool, "edited"), scratch, "edited")
        compare_learn("defs learn decompiled tree", decompiled, lambda tool: fresh_set(scratch, tool, "plain"),
                      scratch, "plain")
    print("all matched" if not failures else "differences: " + ", ".join(failures))
    return 1 if failures else 0


def compare_learn(name, source, before, scratch, label):
    # both tools write into a set with the same path, so their output matches; the sets are kept apart after
    target = scratch / label

    def prepare(tool):
        if target.exists():
            shutil.move(str(target), str(scratch / (label + "-py")))
        before(tool)
        shutil.move(str(scratch / ("%s-%s" % (label, tool))), str(target))
    compare(name + " (output)", ["defs", "learn", str(source), str(target)], prepare)
    differences = tree_differences(scratch / (label + "-py"), target)
    original = tree_differences(REPO / "definitions" / "pal", target)
    print("%-40s %s (%d files changed from definitions/pal)" % (name + " (set)", "same" if not differences else "DIFFERS",
                                                                 len(original)))
    for line in differences:
        print("    " + line)
    if differences:
        failures.append(name + " (set)")


if __name__ == "__main__":
    sys.exit(main())
