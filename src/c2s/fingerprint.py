import hashlib
import json

from c2s import asm, stb

VERSION = 1
JUMPS = ("JMP", "JMP_FALSE", "JMP_TRUE")
CONST_STR = 3


def function_payload(region, code, table, known):
    start = region["offset"] + stb.FUNCDATA_SIZE - code
    texts = {offset - code: text for offset, text in zip(asm.string_offsets(region), region["strings"])}
    pushed = set()
    instructions = []
    for item in region["instructions"]:
        name, arg1, arg2 = item["name"], item["arg1"], item["arg2"]
        if name in JUMPS:
            arg1 = ["jump", arg1 - start]
        elif name == "CALL":
            arg2 = ["call", known.get(arg2, arg2)]
        elif name == "PUSH_CONST" and arg1 == CONST_STR:
            pushed.add(arg2)
            arg2 = ["string", texts.get(arg2, arg2)]
        instructions.append([item["op"], arg1, arg2])
    unused = [text for offset, text in texts.items() if offset not in pushed]
    return {"version": VERSION, "table": table, "arg": region["arg"], "local": region["local"],
            "arg_types": region["arg_types"], "unused": unused, "code": instructions}


def digest(payload):
    text = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    return hashlib.sha1(text.encode("utf-8")).hexdigest()[:16]


def loose_payload(payload):
    code = [[op, arg1, ["string", ""] if isinstance(arg2, list) and arg2[0] == "string" else arg2]
            for op, arg1, arg2 in payload["code"]]
    return dict(payload, unused=[], code=code)


def script_fingerprints(script, table):
    return both_fingerprints(script, table)["exact"]


def both_fingerprints(script, table):
    if script["header"] is None:
        return {"exact": {}, "loose": {}}
    code = script["header"]["code"]
    exact = {}
    loose = {}
    for region in script["regions"]:
        if region["kind"] == "function":
            offset = region["offset"] - code
            exact[offset] = digest(function_payload(region, code, table, exact))
            loose[offset] = digest(loose_payload(function_payload(region, code, table, loose)))
    return {"exact": exact, "loose": loose}
