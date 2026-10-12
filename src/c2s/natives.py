import json
import re
from pathlib import Path

DATA_DIR = Path(__file__).resolve().parent / "tables"

TABLES = (
    {"table": "event", "source": "ps2/src/event_func.cpp", "header": "ps2/include/event_func.hpp", "array": "ext_func_info"},
    {"table": "monster", "source": "ps2/src/runscript_opcodes.cpp", "header": "ps2/include/runscript_opcodes.hpp", "array": "ext_func_info"},
    {"table": "action", "source": "ps2/src/actscript.cpp", "header": "ps2/include/actscript.hpp", "array": "ext_func_info"},
    {"table": "effect", "source": "ps2/src/effscript.cpp", "header": "ps2/include/effscript.hpp", "array": "ext_func_info"},
    {"table": "arg", "source": "ps2/src/event_func.cpp", "header": "ps2/include/event_func.hpp", "array": "esa_ext_func_info"},
)

ACCESSORS = {
    "GetStackInt": "int",
    "GetStackFloat": "float",
    "GetStackString": "string",
    "GetStackVector": "vector",
    "GetStack": "any",
    "SetStack": "out",
    "SetStackVector": "out_vector",
}

ENUM_VALUE = re.compile(r"^\s*([A-Z][A-Z0-9_]*)\s*=\s*(-?(?:0x[0-9a-fA-F]+|\d+))\s*,", re.M)
TABLE_ROW = re.compile(r"\{\s*(\w+)\s*,\s*(\w+)\s*\}")
ACCESSOR_CALL = re.compile(r"\b(" + "|".join(ACCESSORS) + r")\s*\(")


def read_text(path):
    return Path(path).read_text(encoding="utf-8", errors="replace")


def parse_enum_values(text):
    return {name: int(value, 0) for name, value in ENUM_VALUE.findall(text)}


def parse_table_rows(text, array):
    match = re.search(r"static RS_EXTFUNC_INFO " + array + r"\[\d*\] = \{(.*?)\n\};", text, re.S)
    if match is None:
        raise ValueError("table " + array + " not found")
    return [(func, number) for func, number in TABLE_ROW.findall(match.group(1)) if func != "NULL"]


def function_body(text, name):
    match = re.search(r"\bint\s+" + name + r"\s*\(\s*RS_STACKDATA[^)]*\)\s*\{", text)
    if match is None:
        return None
    depth = 1
    position = match.end()
    while depth and position < len(text):
        if text[position] == "{":
            depth += 1
        if text[position] == "}":
            depth -= 1
        position += 1
    return text[match.end():position - 1]


def accessors(body):
    if body is None:
        return None
    return [ACCESSORS[name] for name in ACCESSOR_CALL.findall(body)]


def build_table(root, spec):
    source = read_text(Path(root) / spec["source"])
    values = parse_enum_values(read_text(Path(root) / spec["header"]))
    natives = []
    for func, number in parse_table_rows(source, spec["array"]):
        if number not in values:
            raise ValueError("%s: no value for %s" % (spec["table"], number))
        natives.append((values[number], func, accessors(function_body(source, func))))
    natives = [{"no": no, "name": name, "accessors": calls} for no, name, calls in sorted(natives)]
    return {"table": spec["table"], "source": spec["source"], "array": spec["array"], "natives": natives}


def generate(root, out_dir):
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    tables = {}
    for spec in TABLES:
        table = build_table(root, spec)
        (out / (spec["table"] + ".json")).write_text(json.dumps(table, indent=1) + "\n")
        tables[spec["table"]] = table
    return tables


def load(directory=DATA_DIR):
    tables = {}
    for spec in TABLES:
        path = Path(directory) / (spec["table"] + ".json")
        if path.exists():
            tables[spec["table"]] = json.loads(path.read_text())
    return tables


def numbered(table):
    return {native["no"]: native for native in table["natives"]}


def table_for(path, rules):
    text = str(path).replace("\\", "/")
    for prefix, table in rules:
        if text.startswith(prefix) or "/" + prefix in text:
            return table
    return "event"
