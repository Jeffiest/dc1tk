"""Checks `c2 natives` against `cli.py natives`: both generate the tables from a ChronicleTwo checkout
into scratch folders, and the output, exit code and every file must be the same.

    python rust/parity_natives.py CHRONICLETWO [--shipped]

--shipped also checks the tables equal the shipped ones in src/c2s/tables, less the old_* keys added
there by hand. Run from the repository folder.
"""
import argparse
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
C2 = ROOT / "rust" / "target" / "release" / ("c2.exe" if sys.platform == "win32" else "c2")


def run(command, out):
    start = time.perf_counter()
    # text mode, as parity.py reads output: Python writes \r\n to a Windows console, c2 \n
    done = subprocess.run(command + ["--out", str(out)], capture_output=True, text=True, encoding="utf-8")
    return done, time.perf_counter() - start


def main():
    options = argparse.ArgumentParser()
    options.add_argument("chronicletwo")
    options.add_argument("--shipped", action="store_true")
    args = options.parse_args()
    failures = 0
    with tempfile.TemporaryDirectory() as folder:
        py_out, rs_out = Path(folder) / "py", Path(folder) / "rs"
        py, py_time = run([sys.executable, str(ROOT / "cli.py"), "natives", args.chronicletwo], py_out)
        rs, rs_time = run([str(C2), "natives", args.chronicletwo], rs_out)
        print("python %.2f s, rust %.2f s" % (py_time, rs_time))
        if py.returncode != rs.returncode:
            failures += 1
            print("exit codes differ: %d %d" % (py.returncode, rs.returncode))
        py_text = py.stdout.replace(str(py_out), "OUT")
        rs_text = rs.stdout.replace(str(rs_out), "OUT")
        for name, a, b in (("stdout", py_text, rs_text), ("stderr", py.stderr, rs.stderr)):
            if a != b and not (name == "stderr" and "Traceback" in a and b):
                failures += 1
                print("%s differs:\n--- python\n%s--- rust\n%s" % (name, a, b))
        py_files = sorted(path.name for path in py_out.glob("*")) if py_out.is_dir() else []
        rs_files = sorted(path.name for path in rs_out.glob("*")) if rs_out.is_dir() else []
        if py_files != rs_files:
            failures += 1
            print("files differ: %s %s" % (py_files, rs_files))
        for name in py_files:
            if name in rs_files and (py_out / name).read_bytes() != (rs_out / name).read_bytes():
                failures += 1
                print("%s differs" % name)
        if args.shipped:
            for name in py_files:
                shipped = json.loads((ROOT / "src" / "c2s" / "tables" / name).read_text(encoding="utf-8"))
                shipped = {key: value for key, value in shipped.items() if not key.startswith("old_")}
                if json.loads((rs_out / name).read_text(encoding="utf-8")) != shipped:
                    failures += 1
                    print("%s is not the shipped table" % name)
        print("%d tables, %s" % (len(py_files), "all the same" if not failures else "%d differences" % failures))
        print(py_text, end="")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
