import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "src"))

from c2s import __main__ as c2s_main
from tests import run as test_run


def main():
    if sys.argv[1:2] == ["test"]:
        return test_run.main()
    return c2s_main.main()


if __name__ == "__main__":
    sys.exit(main())
