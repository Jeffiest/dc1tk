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
  python rust/parity.py textures extracted WORK       every .img exported to PNGs by both; c2 building
                                                      each unedited export back to the original; both
                                                      building a set of edited PNGs (--every N)
  python rust/parity.py sounds extracted WORK         the same for every .snd bank as WAVs, with
                                                      synthetic WAVs replacing and adding entries
  python rust/parity.py models extracted WORK         the same for every .chr pack as glTF, with
                                                      meshes moved, scaled and added, bind poses and
                                                      textures changed
                                                      (each with --python-builds: the Python builds
                                                      every unedited export too)

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


# The asset converters by the common format's suffix: the game files each converts and its module.
ASSET_KINDS = {"png": ".img", "wav": ".snd", "gltf": ".chr"}


def asset_module(kind):
    from c2s import models, sounds, textures
    return {"png": textures, "wav": sounds, "gltf": models}[kind]


def game_files(root, kind):
    found = []
    for folder, _, files in os.walk(root):
        for name in files:
            if name.lower().endswith(ASSET_KINDS[kind]):
                found.append((Path(folder) / name).relative_to(root).as_posix())
    return sorted(found)


def write_lines(path, lines):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes("".join(line + "\n" for line in sorted(lines)).encode("utf-8"))


def python_export(kind, root, out, files):
    """What c2 asset-export does: each game file exported to out/<path>.<kind>/, the ones that
    cannot be listed in out/failed.txt."""
    module = asset_module(kind)
    failed = []
    for relative in files:
        try:
            exported = module.export((root / relative).read_bytes())
        except Exception:
            failed.append(relative)
            continue
        folder = out / (relative + "." + kind)
        for name, content in exported.items():
            (folder / name).parent.mkdir(parents=True, exist_ok=True)
            (folder / name).write_bytes(content)
    write_lines(out / "failed.txt", failed)
    return failed


def python_build(kind, root, folders, out, files):
    """What c2 asset-build does: each game file folders has a <path>.<kind>/ for built from it into
    out/<path>, failures (path, tab, message) in out/failed.txt."""
    module = asset_module(kind)
    failed = []
    for relative in files:
        folder = folders / (relative + "." + kind)
        if not folder.is_dir():
            continue
        try:
            built = module.build(folder, (root / relative).read_bytes())
        except Exception as error:
            failed.append("%s\t%s" % (relative, error))
            continue
        (out / relative).parent.mkdir(parents=True, exist_ok=True)
        (out / relative).write_bytes(built)
    write_lines(out / "failed.txt", failed)
    return failed


def compare_builds(py, rs):
    """The built trees compared, failures by path only (the messages are printed, not compared)."""
    def failures(folder):
        lines = (folder / "failed.txt").read_text(encoding="utf-8").splitlines()
        return {line.split("\t")[0]: line for line in lines}
    left, right = failures(py), failures(rs)
    problems = ["failed in %s only: %s" % (name, path) for name, a, b in (("python", left, right), ("rust", right, left))
                for path in sorted(set(a) - set(b))]
    for path in sorted(set(left) & set(right)):
        if left[path] != right[path]:
            print("both failed, other messages:\n  %s\n  %s" % (left[path], right[path]))
    return problems + compare_trees(py, rs)


def fresh(*folders):
    for folder in folders:
        if folder.exists():
            raise SystemExit("%s exists; give an empty work folder" % folder)


def check_asset_exports(kind, args, work):
    """Every game file of a kind exported by both and compared; then c2 building each of the
    Python's unedited exports, which must give the original bytes."""
    root = Path(args.extracted)
    py, rs = work / ("py_" + kind), work / ("rs_" + kind)
    fresh(py, rs)
    files = game_files(root, kind)
    start = time.time()
    failed = python_export(kind, root, py, files)
    py_time = time.time() - start
    out, rs_time = run([C2, "asset-export", kind, root, rs])
    print("%d %s files, %d not exported; python %.1fs, rust %.1fs (%s)" % (len(files), ASSET_KINDS[kind], len(failed), py_time,
                                                                      rs_time, out.strip()))
    problems = compare_trees(py, rs)
    out, rs_time = run([C2, "asset-build", kind, root, py, work / ("rs_" + kind + "_unedited"), "--check"])
    print("rust built the unedited exports in %.1fs: %s" % (rs_time, out.strip().splitlines()[-1]))
    problems += [line for line in out.splitlines() if line.startswith("differs")]
    if args.python_builds:
        start = time.time()
        built = work / ("py_" + kind + "_unedited")
        python_build(kind, root, py, built, files)
        same = sum(1 for path in game_files(built, kind) if (built / path).read_bytes() == (root / path).read_bytes())
        print("python built the unedited exports in %.1fs: %d give the original" % (time.time() - start, same))
    return problems, files, py


def check_asset_edits(kind, args, work, files, edited):
    """Both build the edited folders; the built files compared."""
    root = Path(args.extracted)
    py, rs = work / ("py_" + kind + "_edited"), work / ("rs_" + kind + "_edited")
    fresh(py, rs)
    start = time.time()
    python_build(kind, root, edited, py, files)
    py_time = time.time() - start
    out, rs_time = run([C2, "asset-build", kind, root, edited, rs])
    print("edited: python %.1fs, rust %.1fs (%s)" % (py_time, rs_time, out.strip()))
    return compare_builds(py, rs)


def encoded_png(w, h, rgba, variant):
    """RGBA pixels as a PNG the way image editors write them, in one of 8 variants of colour type,
    bit depth, transparency, row filters (all five, by row) and Adam7 interlacing."""
    import struct
    import zlib
    pixels = [tuple(rgba[i:i + 4]) for i in range(0, len(rgba), 4)]
    colours = sorted(set(pixels))
    # (colour type, bit depth, interlaced)
    ctype, depth, interlace = [(6, 8, 0), (2, 8, 1), (4, 8, 0), (0, 4, 0), (6, 16, 1), (3, 8, 0), (3, 4, 1), (0, 16, 0)][variant]
    if ctype == 3 and len(colours) > (1 << depth):
        ctype, depth = 6, 8
    chunks = []
    if ctype == 3:
        index = {c: i for i, c in enumerate(colours)}
        chunks.append((b"PLTE", b"".join(bytes(c[:3]) for c in colours)))
        chunks.append((b"tRNS", bytes(c[3] for c in colours)))
        samples = [[index[p]] for p in pixels]
    elif ctype == 6:
        samples = [list(p) for p in pixels]
    elif ctype == 4:
        samples = [[p[0], p[3]] for p in pixels]
    elif ctype == 2:
        samples = [list(p[:3]) for p in pixels]
        key = pixels[0][:3]
        chunks.append((b"tRNS", struct.pack(">HHH", *key)))
    else:
        top = (1 << min(depth, 8)) - 1
        samples = [[p[0] * top // 255] for p in pixels]
        chunks.append((b"tRNS", struct.pack(">H", samples[0][0])))
    channels = len(samples[0]) if samples else 1
    bits_pp = depth * channels
    bpp = max(1, bits_pp // 8)

    def row_bytes(row):
        if depth == 16:
            return b"".join(struct.pack(">H", v * 257) for s in row for v in s)
        if depth == 8:
            return bytes(v for s in row for v in s)
        out, acc, bits = bytearray(), 0, 0
        for s in row:
            for v in s:
                acc, bits = (acc << depth) | v, bits + depth
                if bits == 8:
                    out.append(acc)
                    acc, bits = 0, 0
        if bits:
            out.append(acc << (8 - bits))
        return bytes(out)

    def filtered(rows):
        out, prev = bytearray(), None
        for y, line in enumerate(rows):
            prev = prev or bytes(len(line))
            kind = y % 5
            enc = bytearray()
            for i, x in enumerate(line):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                if kind == 1:
                    x -= a
                elif kind == 2:
                    x -= b
                elif kind == 3:
                    x -= (a + b) >> 1
                elif kind == 4:
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    x -= a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                enc.append(x & 255)
            out += bytes([kind]) + enc
            prev = line
        return bytes(out)

    grid = [samples[y * w:(y + 1) * w] for y in range(h)]
    if interlace:
        raw = b""
        for x0, y0, dx, dy in [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]:
            rows = [row_bytes(grid[y][x0::dx]) for y in range(y0, h, dy) if grid[y][x0::dx]]
            raw += filtered(rows)
    else:
        raw = filtered([row_bytes(row) for row in grid])
    out = b"\x89PNG\r\n\x1a\n"
    for tag, body in [(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, ctype, 0, 0, interlace))] + chunks + \
            [(b"IDAT", zlib.compress(raw, 9)), (b"IEND", b"")]:
        out += struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body))
    return out


def edited_pngs(root, exported, edited, every):
    """Deterministic edits of every n-th texture file's PNGs: colours inverted (true colour, so a
    paletted picture is matched or quantised), the palette's red and blue swapped, pixels moved to
    other colours of the palette, and pictures halved in size."""
    from c2s import im3
    folders = sorted(path for path in exported.rglob("*.img.png") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        target = edited / folder.relative_to(exported)
        target.mkdir(parents=True)
        for k, png in enumerate(sorted(folder.glob("*.png"))):
            w, h, mode, data, palette = im3.read_png(png.read_bytes())
            if mode == "P":
                table = [bytes(c) for c in palette] + [b"\0\0\0\xff"] * (256 - len(palette))
                rgba = b"".join(table[i] for i in data)
            else:
                rgba = data
            how = (number + k) % 5
            if how == 0 and w * h <= 128 * 128:
                out = im3.write_png(w, h, "RGBA", bytes(255 - v if i % 4 != 3 else v for i, v in enumerate(rgba)))
            elif how == 1 and mode == "P":
                out = im3.write_png(w, h, "P", data, [(b, g, r, a) for r, g, b, a in palette])
            elif how == 2 and mode == "P":
                out = im3.write_png(w, h, "RGBA", b"".join(table[(v + 1) % len(palette)] for v in data))
            elif how == 3 and w > 1 and h > 1:
                half = b"".join(rgba[4 * (2 * y * w + 2 * x):4 * (2 * y * w + 2 * x) + 4] for y in range(h // 2) for x in range(w // 2))
                out = im3.write_png(w // 2, h // 2, "RGBA", half)
            elif how == 4 and w * h <= 256 * 256:
                out = encoded_png(w, h, rgba, (number // 5 + k) % 8)
            else:
                continue
            (target / png.name).write_bytes(out)
            count += 1
    # sizes and names a build refuses: a swizzled 8-bit picture (IM2) made 8 pixels narrower, so
    # not a multiple of 16 wide, and a PNG naming no picture
    im2 = [folder for folder in folders if (root / folder.relative_to(exported).as_posix()[:-len(".png")]).read_bytes()[:3] == b"IM2"]
    for number, folder in enumerate(im2[:12]):
        target = edited / folder.relative_to(exported)
        target.mkdir(parents=True, exist_ok=True)
        png = sorted(folder.glob("*.png"))[0]
        w, h, mode, data, palette = im3.read_png(png.read_bytes())
        if mode == "P" and w > 8:
            narrow = b"".join(data[y * w:y * w + w - 8] for y in range(h))
            (target / png.name).write_bytes(im3.write_png(w - 8, h, "P", narrow, palette))
            count += 1
        if number % 4 == 0:
            (target / "zz_no_such_picture.png").write_bytes(im3.write_png(4, 4, "RGB", bytes(48)))
            count += 1
    print("%d edited PNGs in %d texture files" % (count, len(folders[::every]) + len(im2[:12])))


def check_textures(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("png", args, work)
    edited = work / "png_edits"
    fresh(edited)
    edited_pngs(Path(args.extracted), exported, edited, args.every)
    return problems + check_asset_edits("png", args, work, files, edited)


def wav_file(rate, channels, bits, values, loop=None, floating=False):
    """A WAV of interleaved sample values (-1.0..1.0), PCM of the bits or float, with a 'smpl' loop
    (start, end) when given."""
    import struct
    if floating:
        body = struct.pack("<%d%s" % (len(values), "f" if bits == 32 else "d"), *values)
    elif bits == 8:
        body = bytes(max(0, min(255, int(128 + v * 127))) for v in values)
    elif bits == 24:
        body = b"".join(int(v * 8388607).to_bytes(3, "little", signed=True) for v in values)
    else:
        body = struct.pack("<%d%s" % (len(values), "h" if bits == 16 else "i"),
                           *(int(v * (32767 if bits == 16 else 2147483647)) for v in values))
    align = channels * bits // 8
    chunks = b"fmt " + struct.pack("<IHHIIHH", 16, 3 if floating else 1, channels, rate, rate * align, align, bits)
    chunks += b"data" + struct.pack("<I", len(body)) + body + (b"\0" if len(body) % 2 else b"")
    if loop:
        smpl = struct.pack("<9I", 0, 0, 0, 60, 0, 0, 0, 1, 0) + struct.pack("<6I", 0, 0, loop[0], loop[1], 0, 0)
        chunks += b"smpl" + struct.pack("<I", len(smpl)) + smpl
    return b"RIFF" + struct.pack("<I", 4 + len(chunks)) + b"WAVE" + chunks


def synthetic_sounds(seed):
    """WAVs to put into a bank: a tone, stereo noise at 96 kHz (resampled to 48 kHz), a looped
    sweep whose loop needs stretching, and 8-bit, 24-bit and float ones at other rates."""
    import math
    state = [seed * 7919 + 1]

    def noise():
        state[0] = (state[0] * 1103515245 + 12345) % (1 << 31)
        return state[0] / (1 << 30) - 1.0
    tone = [0.6 * math.sin(2 * math.pi * 440 * i / 22050) for i in range(6000 + 37 * seed)]
    stereo = [0.3 * noise() for _ in range(2 * 4000)]
    sweep = [0.5 * math.sin(i * i / 9000.0) for i in range(5000)]
    return [
        wav_file(22050, 1, 16, tone),
        wav_file(96000, 2, 16, stereo),
        wav_file(32000, 1, 16, sweep, loop=(1234 + seed, 4999)),
        wav_file(11025, 1, 8, tone[:3000]),
        wav_file(44100, 2, 24, [v * 0.5 for v in tone[:4000] for _ in range(2)]),
        wav_file(16000, 1, 32, tone[:2500], floating=True),
    ]


def edited_wavs(root, exported, edited, every):
    """Deterministic WAV sets for every n-th bank: exported entries replaced by synthetic sounds
    (in place or detached, as the bank's sharing decides), one left as exported, a sequence entry
    replaced, and a new entry added."""
    from c2s import soundbank
    folders = sorted(path for path in exported.rglob("*.snd.wav") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        relative = folder.relative_to(exported).as_posix()[:-len(".wav")]
        bank = soundbank.Bank.from_bytes((root / relative).read_bytes())
        entries = bank.entries()
        target = edited / folder.relative_to(exported)
        target.mkdir(parents=True)
        sounds = synthetic_sounds(number)
        wavs = sorted(folder.glob("*.wav"))
        for k, wav in enumerate(wavs[:5]):
            if k == 1:
                (target / wav.name).write_bytes(wav.read_bytes())
            else:
                (target / wav.name).write_bytes(sounds[(number + k) % len(sounds)])
            count += 1
        exported_numbers = {int(wav.name[:3]) for wav in wavs}
        others = [entry.index for entry in entries if entry.index not in exported_numbers]
        if others:
            (target / ("%03d_sequence.wav" % others[number % len(others)])).write_bytes(sounds[number % len(sounds)])
            count += 1
        (target / ("%03d_new_tone.wav" % len(entries))).write_bytes(sounds[(number + 2) % len(sounds)])
        count += 1
        # ones a build refuses: an entry number past the next one, a WAV format it does not read
        if number % 10 == 3:
            (target / ("%03d.wav" % (len(entries) + 3))).write_bytes(sounds[0])
            count += 1
        if number % 10 == 7 and len(wavs) > 2:
            (target / wavs[2].name).write_bytes(sounds[0][:20] + b"\x02" + sounds[0][21:])
            count += 1
    print("%d edited WAVs in %d banks" % (count, len(folders[::every])))


def check_sounds(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("wav", args, work)
    edited = work / "wav_edits"
    fresh(edited)
    edited_wavs(Path(args.extracted), exported, edited, args.every)
    return problems + check_asset_edits("wav", args, work, files, edited)


def edited_models(exported, edited, every):
    """Deterministic edits of every n-th character pack's export: the positions of every other mesh
    scaled (rigid and skinned ones), one rigid mesh's node moved, and one texture recoloured."""
    import shutil
    import struct
    from c2s import im3
    folders = sorted(path for path in exported.rglob("*.chr.gltf") if path.is_dir())
    count = 0
    for number, folder in enumerate(folders[::every]):
        target = edited / folder.relative_to(exported)
        shutil.copytree(folder, target)
        gltf_path = next(target.glob("*.gltf"))
        gltf = json.loads(gltf_path.read_text(encoding="utf-8"))
        bin_path = target / gltf["buffers"][0]["uri"]
        buffer = bytearray(bin_path.read_bytes())
        for m, mesh in enumerate(gltf.get("meshes", [])):
            if (m + number) % 2:
                continue
            for primitive in mesh["primitives"]:
                accessor = gltf["accessors"][primitive["attributes"]["POSITION"]]
                base = gltf["bufferViews"][accessor["bufferView"]].get("byteOffset", 0) + accessor.get("byteOffset", 0)
                for i in range(accessor["count"]):
                    x, y, z = struct.unpack_from("<3f", buffer, base + 12 * i)
                    struct.pack_into("<3f", buffer, base + 12 * i, x * 1.25, y * 0.75, z + 0.5)
            count += 1
        rigid = [node for node in gltf["nodes"] if "mesh" in node and "skin" not in node]
        changed = False
        if rigid and number % 3 == 0:
            rigid[0]["translation"] = [v + 0.25 for v in rigid[0].get("translation", [0.0, 0.0, 0.0])]
            changed = True
        if rigid and number % 5 == 2:
            # a mesh for a frame the model does not have: a new node under the first frame
            gltf["nodes"].append({"name": "mod_added", "mesh": rigid[-1]["mesh"], "translation": [0.0, 1.0, 0.0]})
            gltf["nodes"][0].setdefault("children", []).append(len(gltf["nodes"]) - 1)
            changed = True
        if gltf.get("skins") and number % 4 == 1:
            # a joint's bind pose moved: its inverse bind matrix's translation changed
            accessor = gltf["accessors"][gltf["skins"][0]["inverseBindMatrices"]]
            base = gltf["bufferViews"][accessor["bufferView"]].get("byteOffset", 0) + accessor.get("byteOffset", 0)
            joint = min(1, accessor["count"] - 1)
            for k in (12, 13, 14):
                (value,) = struct.unpack_from("<f", buffer, base + 64 * joint + 4 * k)
                struct.pack_into("<f", buffer, base + 64 * joint + 4 * k, value + 0.5)
            count += 1
        bin_path.write_bytes(bytes(buffer))
        if changed:
            gltf_path.write_text(json.dumps(gltf, ensure_ascii=False, separators=(",", ":")), encoding="utf-8")
            count += 1
        pngs = sorted((target / "textures").glob("*.png")) if (target / "textures").is_dir() else []
        if pngs:
            png = pngs[number % len(pngs)]
            w, h, mode, data, palette = im3.read_png(png.read_bytes())
            if mode == "P":
                table = [bytes(c) for c in palette] + [b"\0\0\0\xff"] * (256 - len(palette))
                data = b"".join(table[i] for i in data)
            png.write_bytes(im3.write_png(w, h, "RGBA", bytes(255 - v if i % 4 != 3 else v for i, v in enumerate(data))))
            count += 1
    print("%d edits in %d character packs" % (count, len(folders[::every])))


def check_models(args):
    work = Path(args.work)
    problems, files, exported = check_asset_exports("gltf", args, work)
    edited = work / "gltf_edits"
    fresh(edited)
    edited_models(exported, edited, args.every)
    return problems + check_asset_edits("gltf", args, work, files, edited)


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
    for name, every in (("textures", 25), ("sounds", 20), ("models", 20)):
        command = checks.add_parser(name)
        command.add_argument("extracted")
        command.add_argument("work")
        command.add_argument("--every", type=int, default=every, help="edit every n-th file's export (default %d)" % every)
        command.add_argument("--python-builds", action="store_true", help="also build every unedited export with the "
                             "Python and count the ones giving the original")
    args = parser.parse_args()
    if not C2.is_file():
        raise SystemExit("%s is missing; run cargo build --release in rust/" % C2)
    checks = {"extract": check_extract, "write-disc": check_write_disc, "cp932": check_cp932, "disassemble": check_disassemble,
              "decompile": check_decompile, "compile": check_compile,
              "sources": check_sources, "parse": check_parse, "textures": check_textures,
              "sounds": check_sounds, "models": check_models}
    problems = checks[args.check](args)
    for problem in problems[:40]:
        print(problem)
    print("%s: %s" % (args.check, "same bytes" if not problems else "%d differences" % len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
