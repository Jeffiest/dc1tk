"""Checks the Rust port (c2) gives the Python tools' (cli.py) bytes. Each check runs both on the
same input in a work folder and compares; exit status 1 when anything differs.

  python rust/parity.py extract pal.iso WORK          extract with both, compare the trees
  python rust/parity.py write-disc pal.iso WORK       write a set of files touching every branch of
                                                      write_disc with both, compare the images
  python rust/parity.py write-disc pal.iso WORK --mod castle-showcase
                                                      the files cli.py build writes for a mod,
                                                      written by both
  python rust/parity.py cp932 WORK                    every code and character through both codecs
  python rust/parity.py disassemble extracted WORK    every script disassembled by both, and c2
                                                      assembling each listing back to the script
  python rust/parity.py decompile extracted WORK      every script's program tree (before names), as JSON
  python rust/parity.py decompile extracted WORK --source   the same written as C
  python rust/parity.py compile decompiled WORK       every script's resolved program compiled by both
  python rust/parity.py sources extracted WORK        every script decompiled to C one at a time, by both
  python rust/parity.py parse WORK [--data FOLDER]    every source file (decompiled/, mods/ under
                                                      FOLDER; sdk/ and the definitions' headers),
                                                      cut short and with a line gone, and probes of
                                                      #if and the error paths, parsed by both

Run from the repository folder, after cargo build --release in rust/. Images are 4.7 GB each; the
work folder needs room for two.
"""
import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
C2 = REPO / "rust" / "target" / "release" / ("c2.exe" if os.name == "nt" else "c2")
sys.path.insert(0, str(REPO / "src"))


def run(command, **options):
    start = time.time()
    result = subprocess.run([str(part) for part in command], capture_output=True, text=True, encoding="utf-8", **options)
    if result.returncode != 0:
        raise SystemExit("%s failed:\n%s%s" % (" ".join(map(str, command)), result.stdout, result.stderr))
    return result.stdout, time.time() - start


def listing(root):
    out = {}
    for folder, dirs, files in os.walk(root):
        for name in dirs + files:
            path = Path(folder) / name
            out[path.relative_to(root).as_posix()] = path
    return out


def compare_trees(a, b):
    left, right = listing(a), listing(b)
    problems = ["only in %s: %s" % (a.name, path) for path in sorted(set(left) - set(right))]
    problems += ["only in %s: %s" % (b.name, path) for path in sorted(set(right) - set(left))]
    for path in sorted(set(left) & set(right)):
        x, y = left[path], right[path]
        if x.is_dir() != y.is_dir() or (x.is_file() and (x.stat().st_size != y.stat().st_size or x.read_bytes() != y.read_bytes())):
            problems.append("different: %s" % path)
    print("%d entries compared" % len(left))
    return problems


def compare_files(a, b, chunk=1 << 24):
    if a.stat().st_size != b.stat().st_size:
        return ["%s is %d bytes, %s is %d" % (a.name, a.stat().st_size, b.name, b.stat().st_size)]
    with open(a, "rb") as x, open(b, "rb") as y:
        offset = 0
        while True:
            left, right = x.read(chunk), y.read(chunk)
            if left != right:
                at = next(i for i in range(len(left)) if left[i] != right[i])
                return ["%s and %s first differ at byte %d" % (a.name, b.name, offset + at)]
            if not left:
                return []
            offset += len(left)


def check_extract(args):
    work = Path(args.work)
    py, rs = work / "py_extracted", work / "rs_extracted"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    py_out, py_time = run([sys.executable, REPO / "cli.py", "extract", args.iso, py])
    rs_out, rs_time = run([C2, "extract", args.iso, rs])
    print("python %.1fs, rust %.1fs" % (py_time, rs_time))
    problems = [] if py_out.replace(str(py), "OUT").replace(py.name, "OUT") == rs_out.replace(str(rs), "OUT").replace(rs.name, "OUT") \
        else ["printed output differs:\n%s\n%s" % (py_out, rs_out)]
    return problems + compare_trees(py, rs)


def save_files(files, folder):
    folder.mkdir(parents=True, exist_ok=True)
    paths = sorted(files)
    for number, path in enumerate(paths):
        (folder / ("%d.bin" % number)).write_bytes(files[path])
    (folder / "files.json").write_text(json.dumps(paths, ensure_ascii=False), encoding="utf-8")


def synthetic_files(iso_path):
    """Files that take write_disc through each branch: in place (smaller, same size), moved into
    the DMMY filler, past the end of the volume, and new files enough for the index to outgrow its
    sectors."""
    from c2s import archive, iso
    with open(iso_path, "rb") as f:
        disc = archive.open_archive(f)
        filler = iso.find(f, "DMMY")
        by_path = {entry["path"]: entry for entry in disc["entries"]}
        files = {
            "snd2/event/EV_000_130.snd": archive.read_entry(f, disc, by_path["snd2/event/EV_000_130.snd"])[:400000],
            "edit.cfg": archive.read_entry(f, disc, by_path["edit.cfg"]),
            "mainchr/c02fuku1.chr": archive.read_entry(f, disc, by_path["mainchr/c02fuku1.chr"]) + bytes(5000),
            "mods/synthetic/huge.bin": b"\xab" * (filler["size"] + 4096),
        }
        for number in range(120):
            files["mods/synthetic/new_file_%03d.bin" % number] = bytes([number]) * (1000 + number)
    return files


def check_write_disc(args):
    from c2s import __main__ as c2s_main
    from c2s import rebuild
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    folder = work / "files"
    py_iso, rs_iso = work / "py.iso", work / "rs.iso"
    if args.mod:
        captured = {}
        real = rebuild.write_disc

        def capture(iso_path, out_path, files):
            captured.update(files)
            return real(iso_path, out_path, files)

        rebuild.write_disc = capture
        saved = sys.argv
        sys.argv = ["cli.py", "build", args.source, args.iso, "-o", str(py_iso), "--mod", args.mod]
        start = time.time()
        try:
            if c2s_main.main():
                raise SystemExit("cli.py build failed")
        finally:
            sys.argv = saved
            rebuild.write_disc = real
        py_time = time.time() - start
        files = captured
    else:
        files = synthetic_files(args.iso)
        start = time.time()
        rebuild.write_disc(args.iso, str(py_iso), files)
        py_time = time.time() - start
    save_files(files, folder)
    _, rs_time = run([C2, "write-disc", args.iso, rs_iso, folder])
    print("%d files; python %.1fs%s, rust %.1fs" % (len(files), py_time, " (whole build)" if args.mod else "", rs_time))
    problems = compare_files(py_iso, rs_iso)
    if not args.keep:
        py_iso.unlink()
        rs_iso.unlink()
    return problems


def check_disassemble(args):
    """Every script under an extracted disc disassembled by both, compared; then every listing
    assembled by c2 and compared with the script it came from."""
    from c2s import asm, definitions, natives, stb, verify
    work = Path(args.work)
    py, rs = work / "py_asm", work / "rs_asm"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    root = Path(args.extracted)
    defs = definitions.load([definitions.DEFAULT_SET])
    tables = natives.load()
    start = time.time()
    scripts = verify.find_scripts(root)
    for relative in scripts:
        path = root / relative
        table = natives.table_for(path.resolve().as_posix(), defs["tables"])
        target = py / (relative + ".asm")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(asm.disassemble(stb.read(path.read_bytes()), table, tables).encode("utf-8"))
    py_time = time.time() - start
    _, rs_time = run([C2, "disassemble-tree", root, rs])
    print("%d scripts; python %.1fs, rust %.1fs" % (len(scripts), py_time, rs_time))
    problems = compare_trees(py, rs)
    out, rs_time = run_status([C2, "assemble-tree", root, py])
    print("rust assembled python's listings in %.1fs: %s" % (rs_time, out.strip().splitlines()[-1]))
    problems += [line for line in out.splitlines() if line.startswith("differs")]
    return problems


def check_decompile(args):
    """Every script's program tree from decompile.decompile, before names and docs, compared as
    JSON."""
    from c2s import decompile, definitions, names, natives, notes, source, stb, verify
    work = Path(args.work)
    py, rs = work / "py_tree", work / "rs_tree"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    root = Path(args.extracted)
    defs = definitions.load([definitions.DEFAULT_SET])
    tables = natives.load()
    start = time.time()
    scripts = verify.find_scripts(root)
    for relative in scripts:
        path = root / relative
        table = natives.table_for(path.resolve().as_posix(), defs["tables"])
        program = decompile.decompile(stb.read(path.read_bytes()), table, tables)
        if args.named:
            entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
            program = names.apply(program, entries, set(), {})
            program = names.with_script(program, definitions.script_entry(defs, str(path)))
            program = notes.annotate(program, defs["strings"])
        if args.source:
            target, text = py / (relative + ".c"), source.text(program)
        else:
            target, text = py / (relative + ".json"), json.dumps(program, ensure_ascii=False, separators=(",", ":"))
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(text.encode("utf-8"))
    py_time = time.time() - start
    _, rs_time = run([C2, "decompile-dump", root, rs] + (["--source"] if args.source else []) + (["--named"] if args.named else []))
    print("%d scripts; python %.1fs, rust %.1fs" % (len(scripts), py_time, rs_time))
    return compare_trees(py, rs)


def check_compile(args):
    """Every script of a decompiled tree compiled by the Python, with the resolved program its
    compile.to_asm receives saved; c2 compiles the same programs, and the assembly and the script
    bytes are compared (and with the disc's, for the scripts the tree has not changed)."""
    from c2s import compile, definitions, natives, pathnames, rebuild, sdk
    work = Path(args.work)
    trees, py, rs = work / "py_programs", work / "py_out", work / "rs_out"
    unresolved, rs_resolved = work / "py_unresolved", work / "rs_resolved"
    for folder in (trees, py, rs, unresolved, rs_resolved):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    tree = Path(args.source)
    defs = definitions.load([definitions.DEFAULT_SET])
    headers = definitions.header_texts(defs, sdk.SDK_DIR)
    tables = natives.load()
    captured = {}
    real = compile.to_asm

    def capture(program, tables, notes=None):
        text = real(program, tables, notes)
        captured.update(program=program, asm=text)
        return text

    real_resolve = sdk.resolve

    def capture_resolve(program, tables):
        captured.update(unresolved=program)
        return real_resolve(program, tables)

    compile.to_asm = capture
    sdk.resolve = capture_resolve
    start = time.time()
    scripts = rebuild.source_scripts(tree)
    failed = []
    try:
        for relative in scripts:
            captured.clear()
            try:
                data = rebuild.compiled(tree / relative, defs, headers, tables, table_path=pathnames.unnamed(tree, relative))
            except ValueError as error:
                failed.append("%s: %s" % (relative, error))
                continue
            for folder, name, content in ((trees, relative + ".json",
                                           json.dumps(captured["program"], ensure_ascii=False, separators=(",", ":")).encode("utf-8")),
                                          (unresolved, relative + ".json",
                                           json.dumps(captured["unresolved"], ensure_ascii=False, separators=(",", ":")).encode("utf-8")),
                                          (py, relative + ".asm", captured["asm"].encode("utf-8")),
                                          (py, relative + ".stb", data)):
                (folder / name).parent.mkdir(parents=True, exist_ok=True)
                (folder / name).write_bytes(content)
    finally:
        compile.to_asm = real
        sdk.resolve = real_resolve
    py_time = time.time() - start
    out, rs_time = run_status([C2, "to-asm-tree", trees, rs])
    resolved_out, _ = run_status([C2, "resolve-tree", unresolved, rs_resolved])
    print(resolved_out.strip().splitlines()[-1] if resolved_out.strip() else "")
    print("%d scripts (%d the Python could not compile); python %.1fs (parse and resolve included), rust %.1fs (to_asm "
          "and assemble only)" % (len(scripts), len(failed), py_time, rs_time))
    print(out.strip().splitlines()[-1] if out.strip() else "")
    problems = [line for line in (out + resolved_out).splitlines() if line.startswith("failed")]
    return problems + compare_trees(py, rs) + compare_json_trees(trees, rs_resolved)


def compare_json_trees(a, b):
    """Two folders of JSON files with the same names hold the same values (key order aside)."""
    problems = []
    for path in sorted(a.rglob("*.json")):
        other = b / path.relative_to(a)
        if not other.is_file():
            problems.append("missing: %s" % other)
        elif json.loads(path.read_text(encoding="utf-8")) != json.loads(other.read_text(encoding="utf-8")):
            problems.append("different: %s" % path.relative_to(a).as_posix())
    return problems


# #if conditions, each probed in a file of its own.
CONDITIONS = [
    "", "1", "0", "LANGUAGE", "LANGUAGE == 2", "LANGUAGE != LANGUAGE_FRENCH", "defined(LANGUAGE)", "defined LANGUAGE",
    "!defined(X)", "defined ( X ) || 1", "undefined(X)", "1 && 0", "1 || 0", "!0", "!1 == 0", "1 != 2", "!!1", "~0",
    "-1", "+1", "- -1", "1 + 2 * 3 == 7", "(1 + 2) * 3 == 9", "7 / 2 == 3", "-7 / 2 == -3", "7 % -3", "-7 % 3 == 2",
    "4 / 0", "4 % 0", "0 && 1 / 0", "1 << 2", "1 >> 1", "2 ** 3", "1 < 2 < 3", "1 <= 1", "2 >= 3", "1 > 0", "3 & 1",
    "2 | 1", "3 ^ 1", "1 & 1 == 1", "0x10 == 16", "0X1f", "10u", "10UL", "0x10L == 16", "1.5", "1e3", "1.", ".5", "'a'",
    "\"s\"", "1 ? 2 : 3", "1 // 2", "07", "00", "0_1", "1_000", "1__0", "0o17", "0b101", "0b12", "1and 0", "1or 0",
    "0x1for 1", "1andx", "1 # comment", "1 // comment", "(", ")", "(1", "1)", "()", "(1, 2)", "1 2", "", "  ", "\t1",
    "1\x0c+1", "1\x0b+1", "1 \\", "True", "UNDEFINED_NAME", "\u00e91", "1 \u3000", "\uff11", "\u0661", "1 $ 2",
    "!=", "1 ! = 2", "1 !== 1", "1 &&& 1", "1 ||| 1", "1 = 1", "A + A", "SELF", "LOOP_A", "EMPTY", "PAREN",
    "BIG > 1", "99999999999999999999 > 1", "(" * 200 + "1" + ")" * 200, "(" * 201 + "1" + ")" * 201,
    "!" * 300 + "1", "-" * 300 + "1", "&&".join(["1"] * 400), "+".join(["1"] * 300),
    "9007199254740993 / 3 > 0", "LANGUAGE >= LANGUAGE_ENGLISH && LANGUAGE <= LANGUAGE_SPANISH",
]

# Snippets that take the tokenizer and parser down their less trodden paths.
SNIPPETS = [
    "\ufeffint x;", "int x = 1;", "void f() { x = \"a\\x41b\"; }", "void f() { x = \"\\xzz\"; }",
    "void f() { x = \"\\x-1\"; }", "void f() { x = \"\\q\"; }", "void f() { x = \"abc", "void f() { x = \"ab\ncd\"; }",
    "void f() { x = 'a'; y = '\\n'; z = '\\x'; }", "void f() { x = 'ab'; }", "void f() { x = ''; }",
    "void f() { x = '''; }", "void f() { x = \u0661\u0662 + \u0661.\u0665; }", "void f() { x = \u00b2; }",
    "void f() { x = 1.5f + 2f + 3e2 + 0x1fu + 4L; }", "void f() { x = 1e; }", "#pragma assembly\n.code\n",
    "int x;\n#pragma assembly\n", "#pragma magic 0x53423200\n", "#pragma magic 0x5342\n", "#pragma magic 0x53g\n",
    "#pragma magic SB3\n", "#pragma magic S\u00e9\u00e8\n", "#pragma magic \"x\"\n", "#pragma magic\n",
    "#pragma unk_14 0x10\n#pragma globals 3\n", "#pragma unk_14 010\n", "#pragma globals x\n", "#pragma\n",
    "#pragma strings \"a\" \"b\\n\"\nvoid f() {}\n", "#pragma strings a\n", "#pragma padding\nvoid f() {}\n",
    "#pragma padding_of g 00\n#pragma padding_of f\nvoid f() {}\n", "#pragma padding_of \"f\"\n",
    "#pragma target map s51 \"x y\"\n#pragma target script a\n", "#pragma target map\n", "#pragma target nope a\n",
    "#pragma \"name\"\n", "#pragma bogus\n", "#pragma once\n#pragma string_pool\n#pragma natives event\n",
    "#pragma natives \"q\\\"x\"\n", "#pragma natives \"abc\n", "#pragma natives \"a\\zb\"\n", "#pragma natives \"a\\x4\"\n",
    "#pragma override\n#pragma every_frame\nvoid f() {}\nvoid g() {}\n", "#include \"a.h\"\n#include <b.h>\n",
    "#include a.h\n", "#include \"a\" \"b\"\n", "#include \"unterminated\n", "#define\n#define A\n#define B 2 // two\n",
    "#define F(x) x\n", "#define N 4\nvoid f() { int a[N]; int b[M]; int c[] = {1, 2}; }\n", "#define M -1\nvoid f() { int a[M] = {1}; }\n",
    "#define S \"str\"\n#define F 1.5\n#define E (1 + 2)\n#define Q #define\n", "#define X \"abc\n",
    "#define Y asm\n", "#undef X\n#undef\n", "#if 1\nint a;\n#else\nint b;\n#endif\n", "#if 0\n  #endif\nint a;\n#endif\n",
    "#ifdef A\nint a;\n#elif 1\nint b;\n#elif 1/0\n#else\n#endif\n", "#ifndef A\n#define A\n#endif\n#ifdef A\nint a;\n#endif\n",
    "#endif\n", "#else\n", "#elif 1\n", "#if 1\n", "#iffy\n", "# define X 1\n", "#if 0\n#if 1/0\n#endif\n#elif 1/0\n#endif\n",
    "asm f arg 2 {\n  push 1\n  ret\n}\nasm g arg x {\n}\n", "asm f {\n}\n", "asm f arg 1\n", "asm f arg 1 {\n  ret\n",
    "asm f arg 1 { ret }\n} \nint x;", "asm h arg \u0661 {\n}\n", "/* a\nb */ int x;", "/* open", "int x; @",
    "enum e { A, B = 5, C, D = -2, E };\nenum program { P1 = 3, P2 };\n", "enum e { A = x };", "enum e { A, }",
    "PROGRAM(1, main_f);\nPROGRAM(NAME, g);\nMAIN(main_f);\n", "PROGRAM(\"x\", f);", "NEW_FLAG(a);\nNEW_PROGRAM(b);\nNEW_MONSTER_VAR(c);\n",
    "typedef int bgm;\nextern void play(bgm b, enum e x, int *out, OPTIONAL(float f, ...)) NATIVE(5);\n",
    "extern int get(int a) NATIVE(3, 4);\nextern string name(void) NATIVE(1);\nextern any thing(...) NATIVE(0x10);\n",
    "extern int f() NATIVE(x);", "extern void f(void) NATIVE(1)", "extern bogus f() NATIVE(1);",
    "static inline void h(int a, float b) { a = b; }\n", "static inline int h() {}\n", "static int h;",
    "int a, b;\nfloat c;\nstring s;\n", "int a[3];", "int *p;", "int a = 1;", "void v;", "void _f() {}",
    "// doc for f\n// more-\n// over\n//\n// second\nint f(int a, float *b, int c[]) { return a; }\n",
    "// file doc\n// second line\n\nint x;\n", "//only\n", "void f() {\n  // note: kept\n  x = 1;\n  // NOTE: upper\n  // note: last wins\n  y = 2;\n}\n",
    "void f() { int a; float a; }", "void f(int a) { int a; }", "void f() { int a; { int a; } int *b; int *b; }",
    "void f() { if (a) b = 1; else if (c) d = 2; else { e = 3; } }",
    "void f() { while (1) { if (x) continue; break; } }",
    "void f() { for (i = 0, j = 1; i < 10; i++, j--) { if (i) continue; switch (i) { case 1: continue; default: break; } } }",
    "void f() { for (int i = 0; ; ) {} for (;;) ; }", "void f() { do { x++; if (y) continue; } while (x < 3); }",
    "void f() { switch (x) { case 1: case 2: y = 1; break; default: y = 2; } }", "void f() { switch (x) { y = 1; } }",
    "void f() { switch (x) { default: default: } }", "void f() { switch (x) { case A ? 1 : 2: } }",
    "void f() { x = a ? b : c ? d : e; x += 1; x -= 1; x *= 2; x /= 2; x %= 2; x &= 1; x |= 1; x ^= 1; x <<= 1; x >>= 1; }",
    "void f() { x = -a + !b - ~c * &d / *e % (int)f + (float)g; ++x; --x[1]; *p = 1; }",
    "void f() { ++1; }", "void f() { &1; }", "void f() { 1 = 2; }", "void f() { x = sin(1) + cos(y) + g(1, 2,) + h(); }",
    "void f() { x = a[i++]++; y--; print(\"a\", 1); wait; end; skip_end; return; }", "int f() { return 1 + 2; }",
    "void f() { AT(0x10) x = 1; DIRECT { y = 2; z = 3; } AT(1) AT(2) w = 4; }", "void f() { AT(x) y = 1; }",
    "void f() { bgm_file track; bgm_file t2 = 3, t3; enum se_bank b; }", "void f() { void x; }", "void f() { any x; }",
    "void f() { x = 1 }", "void f() { x = ; }", "void f() { #define Z 1\n }", "void f() { x = #include <a>\n; }",
    "void f() { if x; }", "void f() {", "void f(", "void f(int)", "void f(void) {}\nvoid g(void x) {}",
    "int f() { int a[2] = {1, 2, 3}; }", "int f() { int a[] = 1; }", "int f() { int a[x]; int b[3] = {1}; }",
    "void f() { x = 9223372036854775807 + 0x7fffffffffffffff; }",
    "void f() { x = " + "(" * 150 + "1" + ")" * 150 + "; }", "void f() { x = " + "!" * 900 + "1; }",
    "void f() { " + "{" * 300 + "}" * 300 + " }", "void f() { x = " + "(" * 250 + "1" + ")" * 250 + "; }",
    "void f() { x = 1e40; y = 1e400; }", "float x; x;", "int ;", "; int x;", "1;", "\"s\";",
]
# Nesting either side of where Python's recursion limit stops parse.py, called as parse_one calls it.
for depth in (196, 197):
    SNIPPETS.append("void f() { x = " + "(" * depth + "1" + ")" * depth + "; }")
for depth in (983, 984):
    SNIPPETS.append("void f() { x = " + "!" * depth + "1; }")
for depth in (330, 331):
    SNIPPETS.append("void f() { " + "{" * depth + "}" * depth + " }")
for depth in (245, 246):
    SNIPPETS.append("void f() { " + "if (1) " * depth + "x = 1; }")
for depth in (991, 992):
    SNIPPETS.append("#if " + "!" * depth + "1\n#endif\n")
for depth in (990, 991):
    SNIPPETS.append("#define X #if " + "!" * depth + "1\n")


def parse_one(text, defines):
    from c2s import parse
    try:
        return json.dumps(parse.parse(text, defines), ensure_ascii=False, separators=(",", ":"))
    except ValueError as error:
        return json.dumps({"error": str(error)}, ensure_ascii=False, separators=(",", ":"))
    except Exception as error:
        return json.dumps({"crash": type(error).__name__}, separators=(",", ":"))


def check_parse(args):
    """Every source file parsed by parse.parse and by c2-script's parse_dump, compared as JSON:
    the files as they are (headers and mods also with each LANGUAGE a patch is parsed with), the
    first 300 cut at a third and two thirds and with their middle line gone, and probes of #if
    conditions and the error paths."""
    import warnings
    from c2s import patches
    warnings.simplefilter("ignore")
    data = Path(args.data)
    work = Path(args.work)
    folder = work / "parse_inputs"
    rs = work / "rs_parse"
    for path in (folder, rs):
        if path.exists():
            raise SystemExit("%s exists; give an empty work folder" % path)
    folder.mkdir(parents=True)
    files = set()
    for root in (data / "decompiled", REPO / "sdk", REPO / "definitions" / "pal" / "include", data / "mods"):
        files.update(path for path in root.rglob("*") if path.suffix in (".c", ".h") and path.is_file())
    files = sorted(files)
    languages = [patches.NO_LANGUAGE] + sorted(patches.LANGUAGES.values())
    with_defines = [path for path in files if data / "mods" in path.parents or path.suffix == ".h" and data / "decompiled" not in path.parents]
    cases = [(path, None) for path in files]
    for path in with_defines:
        for language in languages:
            cases.append((path, dict({name: str(value) for name, value in patches.LANGUAGES.items()}, LANGUAGE=str(language))))

    def add(name, text, defines=None):
        path = folder / name
        path.write_bytes(text.encode("utf-8"))
        cases.append((path, defines))

    for number, path in enumerate(files[:300]):
        text = path.read_text(encoding="utf-8")
        add("%d_third.c" % number, text[:len(text) // 3])
        add("%d_two_thirds.c" % number, text[:2 * len(text) // 3])
        lines = text.split("\n")
        add("%d_line_gone.c" % number, "\n".join(lines[:len(lines) // 2] + lines[len(lines) // 2 + 1:]))
    probe_defines = {"LANGUAGE": "2", "LANGUAGE_FRENCH": "2", "LANGUAGE_ENGLISH": "1", "LANGUAGE_SPANISH": "5", "A": "1",
                     "SELF": "SELF", "LOOP_A": "LOOP_B", "LOOP_B": "LOOP_A", "EMPTY": "", "PAREN": "(1", "BIG": "0x7fffffffffffffffffffffff",
                     "X": "1 // c"}
    for number, condition in enumerate(CONDITIONS):
        add("if_%d.c" % number, "#if %s\nint a;\n#elif 1\nint b;\n#endif\n" % condition, probe_defines)
    for number, snippet in enumerate(SNIPPETS):
        add("snippet_%d.c" % number, snippet)

    listing = work / "parse_list.txt"
    listing.write_text("".join("%s%s\n" % (path, "" if defines is None else "\t" + json.dumps(defines)) for path, defines in cases),
                       encoding="utf-8")
    texts = [(path.read_text(encoding="utf-8"), defines) for path, defines in cases]
    start = time.time()
    expected = [parse_one(text, defines) for text, defines in texts]
    py_time = time.time() - start
    run(["cargo", "build", "--release", "-q", "-p", "c2-script", "--example", "parse_dump"], cwd=REPO / "rust")
    example = REPO / "rust" / "target" / "release" / "examples" / ("parse_dump.exe" if os.name == "nt" else "parse_dump")
    start = time.time()
    result = subprocess.run([str(example), str(listing), str(rs)], capture_output=True, text=True, encoding="utf-8")
    rs_time = time.time() - start
    if result.returncode != 0:
        raise SystemExit("parse_dump failed:\n%s%s" % (result.stdout, result.stderr))
    problems = []
    crashes = 0
    for number, ((path, defines), want) in enumerate(zip(cases, expected)):
        got = (rs / ("%d.json" % number)).read_text(encoding="utf-8")
        if want.startswith('{"crash":'):
            crashes += 1
            if not got.startswith('{"error":'):
                problems.append("%s%s: python raised %s, rust gave a program" % (path, " %r" % defines if defines else "", want))
        elif got != want:
            problems.append("%s%s differs:\n  python %s\n  rust   %s" % (path, " %r" % defines if defines else "", want[:300], got[:300]))
    errors = sum(want.startswith('{"error":') for want in expected)
    print("%d files parsed (%d programs, %d errors, %d where python raises something other than ValueError and rust gives an error); "
          "python %.1fs, rust %.1fs (%s)" % (len(cases), len(cases) - errors - crashes, errors, crashes, py_time, rs_time, result.stderr.strip()))
    return problems


def check_sources(args):
    """Every script decompiled to C one at a time, as cli.py decompile does for one script (names,
    messages, notes, the SDK's presentation), by both."""
    from c2s import __main__ as c2s_main
    from c2s import decompile, definitions, messages, names, natives, notes, sdk, source, stb, verify
    work = Path(args.work)
    py, rs = work / "py_sources", work / "rs_sources"
    for folder in (py, rs):
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)
    root = Path(args.extracted)
    defs = definitions.load([definitions.DEFAULT_SET])
    tables = natives.load()
    headers = definitions.header_texts(defs, sdk.SDK_DIR)
    contexts = {}
    start = time.time()
    scripts = verify.find_scripts(root)
    for relative in scripts:
        path = str(root / relative)
        # run_decompile's steps for one script, with the header context read once per table
        table = natives.table_for(Path(path).resolve().as_posix(), defs["tables"])
        program = decompile.decompile(stb.read(Path(path).read_bytes()), table, tables)
        if table not in contexts:
            contexts[table] = sdk.with_sound_sets(sdk.header_context(table, headers, tables, defs["include"]), defs)
        found = contexts[table]
        program_names = found["program_names"] if found is not None else {}
        entries = names.with_loose(names.merged(defs["names"]), names.loose_map(program))
        named = names.apply(program, entries, sdk.reserved(found), program_names)
        named = c2s_main.with_messages(named, found, defs, path)
        entry = definitions.script_entry(defs, path)
        scripted = names.with_script(named, entry)
        params = messages.kind_params(named["functions"], found) if found is not None and "asm" not in named else {}
        numbered = sdk.numbered(found, sdk.numbering_of(entry))
        text = source.text(notes.annotate(sdk.present(scripted, numbered, params), defs["strings"]))
        target = py / (relative + ".c")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(text.encode("utf-8"))
    py_time = time.time() - start
    out, rs_time = run_status([C2, "script-sources", root, rs])
    print("%d scripts; python %.1fs, rust %.1fs" % (len(scripts), py_time, rs_time))
    print(out.strip().splitlines()[-1] if out.strip() else "")
    return [line for line in out.splitlines() if line.startswith("failed")] + compare_trees(py, rs)


def run_status(command):
    start = time.time()
    result = subprocess.run([str(part) for part in command], capture_output=True, text=True, encoding="utf-8")
    return result.stdout + result.stderr, time.time() - start


def check_cp932(args):
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    decode, _ = run(["cargo", "run", "--release", "-q", "-p", "c2-disc", "--example", "cp932_table"], cwd=REPO / "rust")
    encode, _ = run(["cargo", "run", "--release", "-q", "-p", "c2-disc", "--example", "cp932_table", "encode"], cwd=REPO / "rust")

    def points(text):
        return " ".join("%04X" % ord(c) for c in text)

    problems = []
    for line in decode.splitlines():
        code, strict, lossy = line.split("\t")
        data = bytes.fromhex(code)
        try:
            expected = points(data.decode("cp932"))
        except UnicodeDecodeError:
            expected = "ERR"
        if expected != strict or points(data.decode("cp932", "replace")) != lossy:
            problems.append("decoding %s differs" % code)
    for line in encode.splitlines():
        point, got = line.split("\t")
        try:
            expected = chr(int(point, 16)).encode("cp932").hex().upper()
        except UnicodeEncodeError:
            expected = "ERR"
        if expected != got:
            problems.append("encoding U+%s differs: python %s, rust %s" % (point, expected, got))
    print("%d codes decoded, %d characters encoded" % (len(decode.splitlines()), len(encode.splitlines())))
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    checks = parser.add_subparsers(dest="check", required=True)
    command = checks.add_parser("extract")
    command.add_argument("iso")
    command.add_argument("work")
    command = checks.add_parser("write-disc")
    command.add_argument("iso")
    command.add_argument("work")
    command.add_argument("--mod", help="the files cli.py build writes for this mod")
    command.add_argument("--source", default="decompiled", help="the decompiled tree build uses")
    command.add_argument("--keep", action="store_true", help="keep the two images")
    command = checks.add_parser("cp932")
    command.add_argument("work")
    command = checks.add_parser("disassemble")
    command.add_argument("extracted")
    command.add_argument("work")
    command = checks.add_parser("sources")
    command.add_argument("extracted")
    command.add_argument("work")
    command = checks.add_parser("compile")
    command.add_argument("source", help="a decompiled tree")
    command.add_argument("work")
    command = checks.add_parser("decompile")
    command.add_argument("extracted")
    command.add_argument("work")
    command.add_argument("--source", action="store_true", help="compare the source text instead of the tree")
    command.add_argument("--named", action="store_true", help="with the definitions' names, script entries and "
                         "translations (without the SDK's names)")
    command = checks.add_parser("parse")
    command.add_argument("work")
    command.add_argument("--data", default=str(REPO), help="the folder holding decompiled/ and mods/")
    args = parser.parse_args()
    if not C2.is_file():
        raise SystemExit("%s is missing; run cargo build --release in rust/" % C2)
    checks = {"extract": check_extract, "write-disc": check_write_disc, "cp932": check_cp932, "disassemble": check_disassemble,
              "decompile": check_decompile, "compile": check_compile,
              "sources": check_sources, "parse": check_parse}
    problems = checks[args.check](args)
    for problem in problems[:40]:
        print(problem)
    print("%s: %s" % (args.check, "same bytes" if not problems else "%d differences" % len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
