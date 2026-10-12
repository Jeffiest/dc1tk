import re
from pathlib import Path

RECORD = re.compile(rb"@(-?\d+)")
UNI = re.compile(r"\[UNI([0-9a-fA-F]{4})\]")
LOCALIZED = re.compile(r"_\d\.txt$")
UNI_REMAP = {"011d": "00e8", "0129": "00ea", "0155": "00e0", "0171": "00fb", "017f": "00f9"}
LATIN_REMAP = {0xBD: "Œ", 0xBE: "œ", 0x9C: "œ"}
MESSAGE_KINDS = ("message",)
NAME_KINDS = ("item_id", "map_id", "npc_id", "chara_pack", "monster_kind")
ITEM = re.compile(r'MES_SYS\s+(\d+)\s*,\s*"([^"]*)"')
MAP = re.compile(r'MAP_NAME\s+"([^"]*)"\s*,\s*"([^"]*)"')
NPC = re.compile(r'NPC_INFO\s+(\d+)\s*,\s*"([^"]*)"\s*,\s*"([^"]*)"')


def record_body(data, start):
    end = start
    while end < len(data) and data[end] != 0:
        if data[end] == 0x0A and data[end + 1:end + 2] == b"@":
            break
        end += 1
    body = data[start:end]
    return body[:-1] if body.endswith(b"\r") else body


def without_comments(body):
    lines = [line for line in body.replace(b"\r\n", b"\n").split(b"\n") if not line.lstrip().startswith(b"//")]
    return b"\n".join(lines).replace(b"\\n", b"\n")


def decoded(body, language):
    if language in (0, 1):
        return body.decode("cp932", "replace")
    return "".join(LATIN_REMAP.get(byte, chr(byte)) for byte in body)


def expanded(text):
    out = []
    position = 0
    for match in UNI.finditer(text):
        code = match.group(1).lower()
        out.append(text[position:match.start()])
        out.append(chr(int(UNI_REMAP.get(code, code), 16)))
        position = match.end()
    out.append(text[position:])
    return "".join(out)


def read_messages(path, language):
    data = Path(path).read_bytes()
    found = {}
    for match in RECORD.finditer(data):
        number = int(match.group(1))
        newline = data.find(b"\n", match.end())
        if number in found or newline < 0:
            continue
        found[number] = expanded(decoded(without_comments(record_body(data, newline + 1)), language)).strip("\n")
    return found


def localized(path, code):
    if LOCALIZED.search(path):
        return path[:-6] + "_%d.txt" % code
    if path.endswith(".txt"):
        return path[:-4] + "_%d.txt" % code
    return path


def kind_positions(declared, kinds):
    return {position for position, param in enumerate(declared["params"]) if param.get("kind") in kinds}


def statements_in(nodes, out):
    for node in nodes:
        out.append(node)
        if node["kind"] == "if":
            statements_in(node["then"], out)
            statements_in(node["else"] or [], out)
        elif node["kind"] == "while":
            statements_in(node["body"], out)
    return out


def calls_in(node):
    if node["kind"] == "native":
        return [("native", node["name"], node["args"])]
    expression = node.get("expr")
    if expression is None:
        return []
    if expression["kind"] == "assign":
        expression = expression["value"]
    if expression["kind"] == "call":
        return [("call", expression["name"], expression["args"])]
    return []


def callee_positions(kind, name, found, params_by_function, kinds=MESSAGE_KINDS):
    if kind == "native":
        declared = found["by_original"].get(name)
        return kind_positions(declared, kinds) if declared is not None else set()
    return params_by_function.get(name, set())


def message_params(functions, found, kinds=MESSAGE_KINDS):
    params_by_function = {}
    changed = True
    while changed:
        changed = False
        for function in functions:
            if function["kind"] != "function":
                continue
            params = [param["name"] for param in function["params"]]
            known = params_by_function.get(function["name"], set())
            for node in statements_in(function["body"], []):
                for kind, name, args in calls_in(node):
                    for position in callee_positions(kind, name, found, params_by_function, kinds):
                        arg = args[position] if position < len(args) else None
                        if arg is not None and arg["kind"] == "var" and arg["name"] in params:
                            index = params.index(arg["name"])
                            if index not in known:
                                known = known | {index}
                                changed = True
            if known:
                params_by_function[function["name"]] = known
    return params_by_function


def string_arg(args, positions):
    for position in sorted(positions):
        if position < len(args) and args[position]["kind"] == "string":
            return args[position]["text"]
    return None


def loaded_files(functions, found):
    files = []
    for function in functions:
        if function["kind"] != "function":
            continue
        directory = ""
        for node in statements_in(function["body"], []):
            if node["kind"] != "native":
                continue
            declared = found["by_original"].get(node["name"])
            if declared is None:
                continue
            changed = string_arg(node["args"], kind_positions(declared, ("directory",)))
            if changed is not None or (kind_positions(declared, ("directory",)) and not node["args"]):
                directory = changed or ""
            loaded = string_arg(node["args"], kind_positions(declared, ("message_file",)))
            if loaded is not None and directory + loaded not in files:
                files.append(directory + loaded)
    return files


def texts_for(files, root, code, cache):
    found = {}
    for file in files:
        path = localized(file, code)
        if path not in cache:
            full = Path(root) / path
            cache[path] = read_messages(full, code) if full.is_file() else {}
        for number, text in cache[path].items():
            found.setdefault(number, text)
    return found


def quoted(text):
    flat = " ".join(part.strip() for part in text.replace("<page>", "\n").split("\n") if part.strip())
    return '"%s"' % flat


def disc_names(root, code):
    found = {"item_id": {}, "map_id": {}, "npc_id": {}, "chara_pack": {}}
    items = Path(root) / ("menu/cfg7/comdatmes%d.cfg" % code)
    if items.is_file():
        for number, name in ITEM.findall(expanded(decoded(items.read_bytes(), code))):
            found["item_id"].setdefault(int(number), name)
    maps = Path(root) / ("map/map%d.cfg" % code)
    if maps.is_file():
        for index, (short, name) in enumerate(MAP.findall(expanded(decoded(maps.read_bytes(), code)))):
            if short:
                found["map_id"].setdefault(index, name)
                found["map_id"].setdefault(short, name)
    people = Path(root) / ("npc%d.cfg" % code)
    if people.is_file():
        for number, name, pack in NPC.findall(expanded(decoded(people.read_bytes(), code))):
            found["npc_id"].setdefault(int(number), name)
            known = found["chara_pack"].get(pack)
            if known is None:
                found["chara_pack"][pack] = name
            elif name not in known.split(" or "):
                found["chara_pack"][pack] = known + " or " + name
    return found


def pack_name(text):
    return text.rsplit("/", 1)[-1].rsplit(".", 1)[0]


def looked_up(arg, table):
    if arg["kind"] == "int":
        return table.get(arg["value"])
    if arg["kind"] == "string":
        return table.get(arg["text"]) or table.get(pack_name(arg["text"]))
    return None


def named_by_enum(arg, kinds, found):
    if arg["kind"] != "int":
        return False
    return any(arg["value"] in members for enum, members in found["enums"].items()
               for kind in kinds if enum == kind or enum.startswith(kind + "_"))


def statement_notes(node, found, lookups):
    lines = []
    for kind, name, args in calls_in(node):
        for kinds, params_by_function, table, quote in lookups:
            for position in sorted(callee_positions(kind, name, found, params_by_function, kinds)):
                if position < len(args) and named_by_enum(args[position], kinds, found):
                    continue
                text = looked_up(args[position], table) if position < len(args) else None
                if text:
                    lines.append(quoted(text) if quote else text)
    return "; ".join(lines)


def annotated(nodes, found, lookups):
    out = []
    for node in nodes:
        if node["kind"] == "if":
            otherwise = None if node["else"] is None else annotated(node["else"], found, lookups)
            node = dict(node, then=annotated(node["then"], found, lookups), **{"else": otherwise})
        elif node["kind"] == "while":
            node = dict(node, body=annotated(node["body"], found, lookups))
        else:
            comment = statement_notes(node, found, lookups)
            if comment:
                node = dict(node, comment="; ".join(part for part in (node.get("comment"), comment) if part))
        out.append(node)
    return out


def lookups_for(params_by_kind, texts, names):
    found = [(MESSAGE_KINDS, params_by_kind.get("message", {}), texts, True)]
    for kind in NAME_KINDS:
        found.append(((kind,), params_by_kind.get(kind, {}), names.get(kind, {}), False))
    return found


def declared_kinds(declared):
    return {position: param.get("kind") or param.get("enum") for position, param in enumerate(declared["params"])
            if (param.get("kind") or param.get("enum")) and not param.get("out")}


def param_kinds(functions, found):
    result = {}
    changed = True
    while changed:
        changed = False
        for function in functions:
            if function["kind"] != "function":
                continue
            params = [param["name"] for param in function["params"]]
            known = dict(result.get(function["name"], {}))
            for node in statements_in(function["body"], []):
                for kind, name, args in calls_in(node):
                    declared = found["by_original"].get(name) if kind == "native" else None
                    callee = declared_kinds(declared) if declared is not None else result.get(name, {})
                    for position, value_kind in callee.items():
                        arg = args[position] if position < len(args) else None
                        if arg is not None and arg["kind"] == "var" and arg["name"] in params:
                            index = params.index(arg["name"])
                            if index not in known:
                                known[index] = value_kind
                                changed = True
            if known:
                result[function["name"]] = known
    return result


def kind_params(functions, found):
    kinds = param_kinds(functions, found)
    params = {}
    for name, positions in kinds.items():
        for position, kind in positions.items():
            group = "message" if kind in MESSAGE_KINDS else kind
            params.setdefault(group, {}).setdefault(name, set()).add(position)
    return params


def annotate_functions(functions, found, params_by_kind, texts, names):
    if found is None or (not texts and not any(names.values())):
        return functions
    lookups = lookups_for(params_by_kind, texts, names)
    return [dict(function, body=annotated(function["body"], found, lookups))
            if function["kind"] == "function" else function for function in functions]
