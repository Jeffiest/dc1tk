import importlib
import sys
import traceback
from pathlib import Path


def test_functions(module):
    return [(name, getattr(module, name)) for name in sorted(vars(module)) if name.startswith("test_")]


def main():
    counts = {"ok": 0, "skip": 0, "FAIL": 0}
    for path in sorted(Path(__file__).parent.glob("test_*.py")):
        module = importlib.import_module("tests." + path.stem)
        for name, function in test_functions(module):
            try:
                outcome = "skip" if function() == "skip" else "ok"
            except Exception:
                outcome = "FAIL"
                traceback.print_exc()
            counts[outcome] += 1
            print("%-5s %s.%s" % (outcome, path.stem, name))
    print()
    print("%d passed, %d skipped, %d failed" % (counts["ok"], counts["skip"], counts["FAIL"]))
    return 1 if counts["FAIL"] else 0


if __name__ == "__main__":
    sys.exit(main())
