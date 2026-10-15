"""Writes tests/cpython.tsv: what CPython gives for the things dc2tk-py reimplements, which
tests/cpython.rs checks the crate against.

Run it with the CPython the port follows (3.13), from the crate's folder:

    python tests/cpython.py > tests/cpython.tsv

Each line is a kind, then tab-separated fields. Floats are their bits in hex (exact, NaNs
included), text is a JSON string (escapes keep tabs, newlines and odd whitespace readable):

    repr        bits  repr(x)
    g           bits  precision  "%.*g" % (precision, x)
    f           bits  precision  "%.*f" % (precision, x)
    sum         bits,bits,...  bits of sum(xs)
    int         text  int(text, 0), or ERR
    float       text  bits of float(text), or ERR
    split       text  text.split() as a JSON list
    strip       text  text.strip()
    space       the code points str.isspace() is true for, as a JSON list
    expandtabs  text  text.expandtabs()
    ratio       a  b  bits of SequenceMatcher(None, a, b).ratio()
    close       word  possibilities (JSON list)  n  cutoff bits  get_close_matches(...) as a JSON list

The random values come from a fixed seed, so the file only changes when this script does.
"""

import difflib
import json
import random
import struct
import sys

out = []


def line(*fields):
    out.append("\t".join(str(field) for field in fields))


def bits(x):
    return f"{struct.unpack('<Q', struct.pack('<d', x))[0]:016X}"


def text(s):
    return json.dumps(s)


def from_bits(value):
    return struct.unpack("<d", struct.pack("<Q", value))[0]


def from_f32_bits(value):
    return struct.unpack("<f", struct.pack("<I", value))[0]


rng = random.Random(20261015)


def finite_doubles(count):
    found = []
    while len(found) < count:
        x = from_bits(rng.getrandbits(64))
        if x == x and abs(x) != float("inf"):
            found.append(x)
    return found


def finite_singles(count):
    found = []
    while len(found) < count:
        x = from_f32_bits(rng.getrandbits(32))
        if x == x and abs(x) != float("inf"):
            found.append(x)
    return found


# floats: the special values, the edges of repr's fixed notation, values the tools print (f32
# widened, as every float of the game's files is), and random doubles of every magnitude
chosen = [
    0.0, -0.0, float("inf"), float("-inf"), float("nan"),
    1.0, -1.0, 0.1, 0.2, 0.3, 1 / 3, 2 / 3, 0.5, 1.5, 2.5, 3.5, -2.5, 100.0, 123456.0, 123.456,
    1e-4, 9.999999999999999e-05, 1e-5, 1.5e-5, 1e15, 1e16, 9999999999999998.0, 1e17, 1e22, 1e23,
    5e-324, 2.2250738585072014e-308, 2.225073858507201e-308, 1.7976931348623157e308,
    3.4028234663852886e38, -1.0728836059570312e-06, 0.1 + 0.2, 1e-7, 123456789012345678.0,
]
# Rust's shortest digits end these ties up, CPython's nearest digits end them even
# (float_repr takes the nearest when they read back)
ties = [0x4055F10680000000, 0x4065C45440000000, 0x43180467B3A7ED6D, 0x42EA8090BB0F6D84, 0x4030113D00000000]
singles = finite_singles(400)
doubles = finite_doubles(400)
scaled = [rng.uniform(-1, 1) * 10 ** rng.randint(-8, 20) for _ in range(200)]
values = chosen + [from_bits(tie) for tie in ties] + singles + doubles + scaled
for x in values:
    line("repr", bits(x), repr(x))
# fewer for the formats, each at every precision
formatted = chosen + [from_bits(tie) for tie in ties] + singles[:20] + doubles[:20] + scaled[:20]
for x in formatted:
    for precision in range(0, 18):
        line("g", bits(x), precision, "%.*g" % (precision, x))
    if abs(x) < 1e30:
        for precision in (0, 1, 2, 3, 6, 9, 17, 20):
            line("f", bits(x), precision, "%.*f" % (precision, x))

# sum(): Neumaier's compensation, the int start, and what infinities and NaNs do to it
sums = [
    [], [-0.0], [-0.0, -0.0], [0.1, 0.2, 0.3], [1e100, 1.0, -1e100], [1e308, 1e308], [1e308, 1e308, -1e308],
    [float("inf"), 1.0], [float("inf"), float("-inf")], [float("nan"), 1.0], [1.0, 1e100, 1.0, -1e100],
    [0.1] * 10, [1e16, 1.0, 1.0, 1.0, 1.0],
]
sums += [finite_singles(rng.randint(2, 12)) for _ in range(60)]
sums += [[rng.uniform(-1e6, 1e6) for _ in range(rng.randint(2, 30))] for _ in range(60)]
for xs in sums:
    line("sum", ",".join(bits(x) for x in xs), bits(float(sum(xs))))

# int(text, 0) and float(text): prefixes, underscores, signs, whitespace, any script's digits
spaces = [" ", "\t", "\n", "\x0b", "\x0c", "\r", "\x1c", "\x1d", "\x1e", "\x1f", "\x85", "\xa0", " ",
          " ", " ", " ", "​", " ", " ", " ", " ", "　", "﻿"]
ints = [
    "0", "00", "0_0", "010", "1_000", "1__0", "_1", "1_", "0x1f", "0X1F", "-0x10", "+0o17", "0o_17", "0b101",
    "-0b_1", "0x", "0x_", "0b2", "0o8", "12", " 12 ", "+ 1", "--1", "1e3", "1.0", "", " ", "abc", "0xg",
    "9223372036854775807", "-9223372036854775808", "9223372036854775808", "-9223372036854775809",
    "170141183460469231731687303715884105727", "170141183460469231731687303715884105728", "9" * 40,
    "0x" + "f" * 32, "١٢", "0x١", "１２", "1٢", "๑๐", "²", "①",
    "1\x00", "12\x7f",
]
ints += [f"{space}12" for space in spaces] + [f"12{space}" for space in spaces]
for s in ints:
    try:
        value = int(s, 0)
    except ValueError:
        value = "ERR"
    line("int", text(s), value)

floats = [
    "1.5", "-1e-3", "1_0.5", "1._5", "1_.5", "1__0", "_1", "1_", ".5", "1.", ".", "e5", "1e", "1e+5", "1E-5",
    "+-1", "1e1_0", "0x10", "inf", "-inf", "+inf", "INF", "iNfInItY", "nan", "-nan", "NaN", "infx", "inf_",
    "in f", "nan(1)", " 1_2 ", "-.5e-3", "١.5", "١٫٥", "１.５", "1e400", "-1e400",
    "1e-400", "0.1", "3.4028234663852886e38", "123456789012345678901234567890", "", " ", "1 2", "1,5",
]
floats += [f"{space}1.5" for space in spaces] + [f"1.5{space}" for space in spaces]
for s in floats:
    try:
        value = bits(float(s))
    except ValueError:
        value = "ERR"
    line("float", text(s), value)

# str.split(), str.strip() and str.isspace()
pieces = spaces + ["a", "bc", "あ", "é", "", "  "]
for _ in range(80):
    s = "".join(rng.choice(pieces) for _ in range(rng.randint(0, 8)))
    line("split", text(s), json.dumps(s.split()))
    line("strip", text(s), text(s.strip()))
line("space", json.dumps([point for point in range(sys.maxunicode + 1) if chr(point).isspace()]))

# str.expandtabs()
tabbed = ["", "\t", "a\tb", "12345678\tx", "1234567\tx", "\t\t", "ab\ncd\te", "ab\rcd\te", "é\tx",
          "あ\tx", "a\x0bb\tc", "a\r\n\tb", "x" * 9 + "\t|"]
for s in tabbed:
    line("expandtabs", text(s), text(s.expandtabs()))

# difflib: short words, and long ones where autojunk (200 elements or more) drops popular ones
words = ["ape", "apple", "peach", "puppy", "appel", "flor_start", "floor_start", "floor_end", "map_start",
         "start", "", "a", "ab", "ba", "abcd", "bcda", "あい", "いあ"]
for a in words:
    for b in words:
        line("ratio", text(a), text(b), bits(difflib.SequenceMatcher(None, a, b).ratio()))
for _ in range(50):
    alphabet = "abcdefghijklmnopqrstuvwxyz"[: rng.randint(2, 26)]
    b = "".join(rng.choice(alphabet) for _ in range(rng.randint(150, 300)))
    a = "".join(rng.choice(alphabet) for _ in range(rng.randint(1, 300)))
    if rng.random() < 0.5:
        # a copy of b with a few changes, so long matches cross popular elements
        a = list(b)
        for _ in range(rng.randint(1, 20)):
            a[rng.randrange(len(a))] = rng.choice(alphabet)
        a = "".join(a)
    line("ratio", text(a), text(b), bits(difflib.SequenceMatcher(None, a, b).ratio()))
# autojunk makes a and b popular in this b (more than 3 of 201), X, Y and Z are not: the longest
# match YZ is extended over the popular "ab" before it, and then over the X and b before those
a, b = "bXabYZ", "XbXabYZ" + "ab" * 97
line("ratio", text(a), text(b), bits(difflib.SequenceMatcher(None, a, b).ratio()))
# a few popular letters among many rare characters, a a changed slice of b: the longest match of
# rare characters is extended over popular letters, and over rare characters beyond them
rare = [chr(0x4E00 + i) for i in range(400)]
for _ in range(40):
    common = "abc"[: rng.randint(1, 3)]
    share = rng.choice([0.3, 0.5, 0.7])
    b = "".join(rng.choice(common) if rng.random() < share else rng.choice(rare) for _ in range(rng.randint(200, 220)))
    start = rng.randrange(0, 150)
    a = list(b[start : start + rng.randint(5, 60)])
    for _ in range(rng.randint(0, 6)):
        a[rng.randrange(len(a))] = rng.choice(common) if rng.random() < 0.5 else rng.choice(rare)
    a = "".join(a)
    line("ratio", text(a), text(b), bits(difflib.SequenceMatcher(None, a, b).ratio()))
for word in ["appel", "flor_start", "pech", "x", ""]:
    for n, cutoff in [(3, 0.6), (1, 0.6), (5, 0.0), (3, 0.9)]:
        found = difflib.get_close_matches(word, words, n, cutoff)
        line("close", text(word), json.dumps(words), n, bits(cutoff), json.dumps(found))

sys.stdout.reconfigure(newline="\n", encoding="ascii")
print("\n".join(out))
