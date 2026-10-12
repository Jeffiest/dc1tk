from c2s import stack, stb
from tests import build


def item(name, arg1=0, arg2=0):
    return {"name": name, "arg1": arg1, "arg2": arg2}


def script(instructions):
    return {"header": {"code": 0}, "regions": [{"kind": "function", "offset": 0, "arg": 0, "instructions": instructions}]}


def test_sample_native_call_skips_script_call_arguments():
    data = build.sample_script()
    main = stb.read(data)["regions"][2]
    assert stack.read_calls(data) == [{"function": main["offset"], "index": 6, "args": 2, "no": 12}]


def test_indexed_address_keeps_depth():
    instructions = [item("PUSH_CONST", 1, 40), item("PUSH_CONST", 1, 0), item("LOAD_ADDR", 3, 0x2), item("EXT", 2)]
    assert stack.native_calls(script(instructions))[0]["no"] == 40


def test_computed_native_number_is_unknown():
    instructions = [item("LOAD", 0, 1), item("PUSH_CONST", 1, 9), item("EXT", 2)]
    assert stack.native_calls(script(instructions))[0]["no"] is None


def test_grouped_native_number_is_folded():
    instructions = [item("PUSH_CONST", 1, 70), item("PUSH_CONST", 1, 5), item("ADD"), item("LOAD", 0, 1), item("EXT", 2)]
    assert stack.native_calls(script(instructions))[0]["no"] == 75
