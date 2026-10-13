"""Checks the Rust port (c2) gives the Python tools' (cli.py) bytes. Each check runs both on the
same input in a work folder and compares; exit status 1 when anything differs.

  python rust/parity.py extract pal.iso WORK          extract with both, compare the trees
  python rust/parity.py write-disc pal.iso WORK       write a set of files touching every branch of
                                                      write_disc with both, compare the images
  python rust/parity.py write-disc pal.iso WORK --mod castle-showcase
                                                      the files cli.py build writes for a mod,
                                                      written by both
  python rust/parity.py cp932 WORK                    every code and character through both codecs

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
    args = parser.parse_args()
    if not C2.is_file():
        raise SystemExit("%s is missing; run cargo build --release in rust/" % C2)
    problems = {"extract": check_extract, "write-disc": check_write_disc, "cp932": check_cp932}[args.check](args)
    for problem in problems[:40]:
        print(problem)
    print("%s: %s" % (args.check, "same bytes" if not problems else "%d differences" % len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
