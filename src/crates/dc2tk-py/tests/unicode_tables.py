"""Writes src/unicode_tables.rs: the code points CPython's str methods are true for.

Run it with the CPython whose str methods the port follows (the file names its version), from
the crate's folder:

    python tests/unicode_tables.py > src/unicode_tables.rs

The output is laid out as rustfmt leaves it, so `cargo fmt` does not change it.
"""

import platform
import sys
import unicodedata

# rustfmt (max_width = 120) fills a line of short array items to at most 119 columns
WIDTH = 119


def runs(test):
    """The code points test is true for, as sorted pairs of first and last."""
    pairs = []
    start = None
    for point in range(sys.maxunicode + 2):
        true = point <= sys.maxunicode and test(chr(point))
        if true and start is None:
            start = point
        elif not true and start is not None:
            pairs += [start, point - 1]
            start = None
    return pairs


def decimal_zeros():
    """The first code point of each run of ten decimal digits."""
    return [point for point in range(sys.maxunicode + 1) if unicodedata.decimal(chr(point), None) == 0]


def table(name, values):
    lines = [f"pub(crate) const {name}: [u32; {len(values)}] = ["]
    line = ""
    for value in values:
        item = f"0x{value:X},"
        if line and len(line) + 1 + len(item) > WIDTH:
            lines.append(line)
            line = ""
        line = f"{line} {item}" if line else f"    {item}"
    lines.append(line)
    lines.append("];")
    return "\n".join(lines)


def main():
    sys.stdout.reconfigure(newline="\n")
    print(
        f"// Generated from CPython {platform.python_version()}'s str methods (Unicode {unicodedata.unidata_version}): "
        "the code points each one is true for, as"
    )
    print("// sorted pairs of first and last code point; DECIMAL_ZEROS starts each run of ten decimal digits.")
    print("// Written by tests/unicode_tables.py.")
    tables = [
        ("ALPHA", runs(str.isalpha)),
        ("DECIMAL", runs(str.isdecimal)),
        ("DIGIT", runs(str.isdigit)),
        ("NUMERIC", runs(str.isnumeric)),
        ("PRINTABLE", runs(str.isprintable)),
        ("DECIMAL_ZEROS", decimal_zeros()),
    ]
    for name, values in tables:
        print()
        print(table(name, values))


main()
