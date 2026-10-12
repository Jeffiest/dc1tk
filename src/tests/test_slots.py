from c2s import slots


def load(slot, pack):
    return {"kind": "native", "name": "_LOAD_CHARA", "args": [{"kind": "int", "value": 4}, {"kind": "string", "text": pack},
                                                             {"kind": "string", "text": "info.cfg"}, {"kind": "int", "value": slot}]}


def program(*loads):
    return {"functions": [{"kind": "function", "body": list(loads)}]}


def test_slots_a_folder_always_fills_with_one_character_are_named():
    found = {}
    slots.survey(found, "map/m/m01/m01.stb", program(load(32, "event/e109/e109_starbre.chr"), load(37, "chara/c01_base.chr"),
                                                     load(9, "chara/p01_20a.chr"), load(40, "a.chr")))
    slots.survey(found, "map/m/m01/m01b.stb", program(load(32, "event/e110/e110_starbre.chr"), load(37, "chara/c01_base.chr"),
                                                      load(40, "b.chr")))
    names = slots.names(found, {"chara/p01_20a.chr": "Giles or Apia"})
    # 32 holds Star Breaker from two events; 9 is a shared villager model and 40 two characters
    assert names == {"map/m/m01": {32: "CHARA_M01_STARBRE", 37: "CHARA_M01_MAX"}}
    assert "CHARA_M01_MAX = 37," in slots.header_text(names)
    context = slots.in_folder({"enums": {}}, "map/m/m01/m01.stb", names)
    assert context["enums"]["chara_slot_folder"] == {32: "CHARA_M01_STARBRE", 37: "CHARA_M01_MAX"}
    assert slots.in_folder({"enums": {}}, "map/m/m02/m02.stb", names) == {"enums": {}}
