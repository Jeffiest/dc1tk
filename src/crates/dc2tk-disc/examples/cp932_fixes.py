"""Writes src/cp932_fixes.rs: the characters whose cp932 bytes in Python's codec differ from
WHATWG Shift_JIS's (encoding_rs), which text::encode corrects.

From the crate's folder:

    cargo run -q --example cp932_table whatwg > whatwg.txt
    python examples/cp932_fixes.py whatwg.txt > src/cp932_fixes.rs

whatwg.txt (delete it after) lists every character below U+10000 with encoding_rs's bytes
("code point<TAB>hex bytes or ERR"). Run it with the CPython whose codec the tools follow; the
table written this way is the one checked in, byte for byte. The four private use characters cp932 gives single bytes (U+F8F0 to U+F8F3) are
left out: text.rs handles them with the single bytes it decodes.
"""

import sys

NO_BYTES = 0xFFFF
PRIVATE_SINGLES = range(0xF8F0, 0xF8F4)


def python_bytes(point):
    try:
        return chr(point).encode("cp932")
    except UnicodeEncodeError:
        return None


def fixes(listing):
    found = []
    for line in listing:
        point, hex_bytes = line.rstrip("\n").split("\t")
        point = int(point, 16)
        whatwg = None if hex_bytes == "ERR" else bytes.fromhex(hex_bytes)
        python = python_bytes(point)
        if python == whatwg or point in PRIVATE_SINGLES:
            continue
        found.append((point, NO_BYTES if python is None else int.from_bytes(python, "big")))
    return sorted(found)


def main():
    with open(sys.argv[1], encoding="ascii") as listing:
        found = fixes(listing)
    sys.stdout.reconfigure(newline="\n")
    print("// Generated from Python's cp932 codec: the characters whose cp932 encoding differs from what")
    print("// WHATWG Shift_JIS (encoding_rs) gives, with Python's bytes as a big-endian u16 (one byte below")
    print("// 0x100) or NO_BYTES where Python has none. Sorted by code point. Written by examples/cp932_fixes.py.")
    print()
    print(f"pub(crate) const NO_BYTES: u16 = 0x{NO_BYTES:X};")
    print()
    print(f"pub(crate) const ENCODE_FIXES: [(u16, u16); {len(found)}] = [")
    for point, value in found:
        print(f"    (0x{point:04X}, 0x{value:04X}),")
    print("];")


main()
