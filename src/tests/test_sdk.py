import re

from c2s import compile, decompile, library, names, natives, parse, sdk, source, stb

EVENT_HEADER = """#ifndef C2S_EVENT_H
#define C2S_EVENT_H
#include <c2s.h>
#pragma natives event

enum program {
    init = 100,
};

// Reads an event flag.
extern int get_flag(int flag) NATIVE(14);
extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);

static inline void get_flag_twice(int flag, int *value)
{
    *value = get_flag(flag);
    *value = get_flag(*value);
}

static inline void set_actor_face(int actor, string face, string eyes)
{
    set_actor_texture_anim(actor, 0);
    set_actor_texture_anim(actor, 1, face);
    set_actor_texture_anim(actor, 1, eyes);
}
#endif
"""

MONSTER_HEADER = """#include <c2s.h>
#pragma natives monster

extern float get_target_angle(void) NATIVE(70, 15);
"""

HEADERS = {"event.h": EVENT_HEADER, "monster.h": MONSTER_HEADER, "c2s.h": "#define NATIVE(...)\n"}

EVENT = """#include <event.h>

PROGRAM(init, on_init);
MAIN(on_init);

void on_init(void)
{
    int flag;

    flag = get_flag(12);
    set_actor_face(flag, "smile", "blink");
    set_actor_texture_anim(1, 0);
}
"""

MONSTER = """#include <monster.h>

PROGRAM(100, program_100);
MAIN(program_100);

void program_100(void)
{
    float target_angle;

    target_angle = get_target_angle();
    AT(60) target_angle = get_target_angle();
    DIRECT target_angle = get_target_angle();
    AT(70) _GET_TARGET_ANGLE(&target_angle);
}
"""


def build(text):
    program = library.expand(parse.parse(text), {}, HEADERS)
    return compile.compile_program(program, natives.load())


def readable(data, table):
    tables = natives.load()
    found = sdk.header_context(table, HEADERS, tables)
    program = decompile.decompile(stb.read(data), table, tables)
    named = names.apply(program, {}, sdk.reserved(found), found["program_names"])
    return source.text(sdk.present(named, found))


def test_sdk_names_round_trip_through_bytes():
    data = build(EVENT)
    assert readable(data, "event") == EVENT


def test_inline_helpers_expand_to_native_calls():
    plain = EVENT.replace('    set_actor_face(flag, "smile", "blink");\n',
                          '    _OBJS_TEX_ANIME(flag, 0);\n    _OBJS_TEX_ANIME(flag, 1, "smile");\n'
                          '    _OBJS_TEX_ANIME(flag, 1, "blink");\n')
    assert build(plain) == build(EVENT)


def test_default_groups_and_explicit_forms():
    data = build(MONSTER)
    text = readable(data, "monster")
    assert ("    target_angle = get_target_angle();\n    AT(60) target_angle = get_target_angle();\n"
            "    DIRECT target_angle = get_target_angle();\n    target_angle = get_target_angle();\n") in text
    assert build(text) == data


def test_original_names_still_compile():
    original = EVENT.replace("flag = get_flag(12);", "_GET_FLAG(12, &flag);")
    assert build(original) == build(EVENT)


def test_program_names_come_from_the_header():
    assert build(EVENT.replace("PROGRAM(init, on_init);", "PROGRAM(100, on_init);")) == build(EVENT)
    try:
        build(EVENT.replace("PROGRAM(init, on_init);", "PROGRAM(start, on_init);"))
    except ValueError as error:
        assert "start" in str(error)
    else:
        raise AssertionError("compiled an unknown program name")


def test_locals_are_named_after_results():
    renamed = re.sub(r"\bflag\b", "v0", EVENT)
    assert readable(build(renamed), "event") == EVENT


def test_natives_in_expressions_are_called_first():
    build(EVENT.replace("flag = get_flag(12);", "flag = get_flag(12) + 1;"))
    try:
        build(EVENT.replace("flag = get_flag(12);", "flag = set_actor_texture_anim(1, 0);"))
    except ValueError as error:
        assert "returns nothing" in str(error)
    else:
        raise AssertionError("a native without a result was given a value")


def test_header_declarations_parse():
    header = parse.parse(EVENT_HEADER + "extern void get_pos(OPTIONAL(float *x, ...)) NATIVE(70, 15);\n")
    natives_declared = header["declarations"]["natives"]
    assert natives_declared[0]["returns"] == "int"
    assert natives_declared[0]["params"][-1] == {"name": "flag", "type": "int", "out": True, "optional": False, "result": True}
    assert natives_declared[1]["params"][2] == {"name": "anim_name", "type": "string", "out": False, "optional": True}
    assert natives_declared[2]["base"] == 70 and natives_declared[2]["no"] == 85 and natives_declared[2]["variadic"]
    assert natives_declared[2]["params"][0]["out"]
    assert header["declarations"]["programs"] == [{"name": "init", "no": 100}]


def test_pointer_parameters_of_helpers():
    plain = EVENT.replace("    flag = get_flag(12);\n", "    flag = get_flag(12);\n    flag = get_flag(flag);\n")
    folded = EVENT.replace("    flag = get_flag(12);\n", "    get_flag_twice(12, &flag);\n")
    assert build(plain) == build(folded)
    assert readable(build(plain), "event") == folded


def test_declarations_print_as_c():
    header = parse.parse(EVENT_HEADER)["declarations"]
    assert source.native_declaration(header["natives"][0]) == "extern int get_flag(int flag) NATIVE(14);"
    assert (source.native_declaration(header["natives"][1])
            == "extern void set_actor_texture_anim(int actor, int on, OPTIONAL(string anim_name)) NATIVE(471);")
    found = sdk.context(header, "event", natives.load())
    text = "\n".join(source.inline_text(sdk.present_inline(found["inlines"]["get_flag_twice"], found)))
    assert text == ("static inline void get_flag_twice(int flag, int *value)\n{\n"
                    "    *value = get_flag(flag);\n    *value = get_flag(*value);\n}")


CONSTANT_HEADER = """#include <c2s.h>
#pragma natives event

#define PI 3.1415927

enum motion_flags {
    MOTION_ONCE = 2,
    MOTION_LOOP = 4,
};

extern void set_actor_motion(int actor, string motion, enum motion_flags flags) NATIVE(465);
extern void wait_frames_native(int frames) NATIVE(14);
"""

CONSTANTS = """#include <event.h>

PROGRAM(100, program_100);
MAIN(program_100);

void program_100(void)
{
    float angle;

    angle = PI * 2.0;
    set_actor_motion(0, "walk", MOTION_LOOP | MOTION_ONCE);
    set_actor_motion(0, "walk", 4);
    set_actor_motion(0, "walk", 6);
    wait_frames_native(4);
}
"""


def constant_build(text):
    program = library.expand(parse.parse(text), {}, {"event.h": CONSTANT_HEADER})
    return compile.compile_program(program, natives.load())


def test_constants_compile_to_their_literals():
    literal = (CONSTANTS.replace("PI * 2.0", "3.1415927 * 2.0").replace("MOTION_LOOP | MOTION_ONCE", "4 | 2"))
    assert constant_build(CONSTANTS) == constant_build(literal)


def test_decompiler_names_only_enum_typed_literals():
    tables = natives.load()
    found = sdk.header_context("event", {"event.h": CONSTANT_HEADER}, tables)
    program = decompile.decompile(stb.read(constant_build(CONSTANTS)), "event", tables)
    text = source.text(sdk.present(names.apply(program, {}, sdk.reserved(found), {}), found))
    assert "    set_actor_motion(0, \"walk\", MOTION_LOOP | MOTION_ONCE);\n" in text
    assert "    set_actor_motion(0, \"walk\", MOTION_LOOP);\n" in text
    assert "    set_actor_motion(0, \"walk\", 6);\n" in text
    assert "    wait_frames_native(4);\n" in text
    assert " = PI * 2.0;\n" in text
    assert constant_build(text) == constant_build(CONSTANTS)


def test_locals_shadow_constants():
    shadowed = CONSTANTS.replace("    float angle;\n", "    float angle;\n    int MOTION_LOOP;\n")
    shadowed = shadowed.replace("set_actor_motion(0, \"walk\", 4);", "set_actor_motion(0, \"walk\", MOTION_LOOP);")
    script = stb.read(constant_build(shadowed))
    loads = [item for item in script["regions"][2]["instructions"] if item["name"] == "LOAD"]
    assert any(item["arg1"] == 1 for item in loads)


def test_raw_numbered_calls_name_the_native_they_were_written_for():
    header = MONSTER_HEADER + "extern void get_monster_var2(int index, OPTIONAL(int *value)) NATIVE(35, 25);\n"
    headers = dict(HEADERS, **{"monster.h": header})
    text = MONSTER.replace("    AT(60) target_angle = get_target_angle();\n", "    DIRECT get_monster_var2(1);\n")
    data = compile.compile_program(library.expand(parse.parse(text), {}, headers), natives.load())
    tables = natives.load()
    found = sdk.header_context("monster", headers, tables)
    program = decompile.decompile(stb.read(data), "monster", tables)
    shown = source.text(sdk.present(names.apply(program, {}, sdk.reserved(found), found["program_names"]), found))
    # raw 60 is get_target_angle in the table the call was written for
    assert "    DIRECT get_monster_var2(1);  // old numbering, meant as get_target_angle\n" in shown
    assert "    DIRECT target_angle = get_target_angle();\n" in shown


def test_scripts_built_for_the_old_table_read_current_groups_as_old():
    header = (MONSTER_HEADER + "extern void set_visible_monster_limit(int count, OPTIONAL(int extra2)) NATIVE(110, 9);\n"
              "extern void set_mask_flag(int mask, int on) NATIVE(120, 9);\n")
    headers = dict(HEADERS, **{"monster.h": header})
    text = MONSTER.replace("    AT(60) target_angle = get_target_angle();\n", "    set_visible_monster_limit(4, 1);\n")
    data = compile.compile_program(library.expand(parse.parse(text), {}, headers), natives.load())
    tables = natives.load()
    found = sdk.header_context("monster", headers, tables)
    program = decompile.decompile(stb.read(data), "monster", tables)
    named = names.apply(program, {}, sdk.reserved(found), found["program_names"])
    assert "old numbering" not in source.text(sdk.present(named, found))
    shown = source.text(sdk.present(named, sdk.numbered(found, "old")))
    assert "    set_visible_monster_limit(4, 1);  // old numbering, meant as set_mask_flag\n" in shown


def test_sound_entries_take_the_names_of_the_bank_their_id_came_from():
    header = EVENT_HEADER.replace("#endif", "extern int get_system_sound_id(void) NATIVE(620);\n"
                                  "extern void play_se(int sound_id, int se_no, OPTIONAL(int volume, int pan)) NATIVE(602);\n"
                                  "enum system_se_pal {\n    SE_TREASURE_GET = 18,\n};\n#endif")
    headers = dict(HEADERS, **{"event.h": header})
    text = EVENT.replace("    set_actor_texture_anim(1, 0);\n",
                         "    flag = get_system_sound_id();\n    play_se(flag, 18);\n    flag = 3;\n    play_se(flag, 18);\n")
    data = compile.compile_program(library.expand(parse.parse(text), {}, headers), natives.load())
    tables = natives.load()
    found = sdk.header_context("event", headers, tables)
    program = decompile.decompile(stb.read(data), "event", tables)
    shown = source.text(sdk.present(names.apply(program, {}, sdk.reserved(found), found["program_names"]), found))
    assert "play_se(flag, SE_TREASURE_GET);" in shown
    assert "play_se(flag, 18);" in shown


def test_unregistered_numbers_of_an_old_table_name_their_native():
    found = {"by_original": {}, "old_direct": {80: 76}, "raw_table": True, "by_number": {76: "add_motion_se"},
             "old_groups": {}}
    assert sdk.meant_native({"name": "_EXT_80", "base": None}, found) == "add_motion_se"
    assert sdk.meant_native({"name": "_EXT_80", "base": None}, dict(found, raw_table=False)) is None


def test_entries_of_loaded_banks_take_their_bank_names():
    found = {"enums": {"se_sp_000_pal": {0: "SE_SP_000_FANFARE"}, "se_ev_283_000_pal": {3: "SE_EV_283_000_DOOR"}},
             "constants": {}, "sound_set_banks": {2237: "EV_283_000"}}
    var = lambda name: {"kind": "var", "name": name}
    num = lambda value: {"kind": "int", "value": value}
    body = [{"kind": "native", "name": "load_sound", "args": [var("SND_PORT_MENU"), {"kind": "string", "text": "snd2/sp/sp_000.snd"}],
             "result": var("menu")},
            {"kind": "native", "name": "play_se", "args": [var("menu"), num(0)]},
            {"kind": "native", "name": "load_sound", "args": [num(2237), {"kind": "addr", "target": var("scene")}]},
            {"kind": "native", "name": "play_se", "args": [var("scene"), num(3)]},
            {"kind": "native", "name": "get_sound_port_bank", "args": [var("SND_PORT_EVENT")], "result": var("port")},
            {"kind": "native", "name": "play_actor_se", "args": [num(1), var("port"), num(3)]}]
    shown = sdk.named_sounds(body, found, {})
    assert shown[1]["args"][1] == var("SE_SP_000_FANFARE")
    assert shown[3]["args"][1] == var("SE_EV_283_000_DOOR")
    assert shown[5]["args"][2] == var("SE_EV_283_000_DOOR")


def test_banks_reach_entries_through_parameters_and_out_parameters():
    found = {"enums": {"se_sp_000_pal": {0: "SE_SP_000_FANFARE"}}, "constants": {}, "sound_set_banks": {}}
    var = lambda name: {"kind": "var", "name": name}
    load = {"kind": "native", "name": "load_sound",
            "args": [var("SND_PORT_MENU"), {"kind": "string", "text": "snd2/sp/sp_000.snd"}, var("out")]}
    loader = {"kind": "function", "name": "load_fanfare", "params": [{"name": "out", "array": True}], "locals": [],
              "body": [load]}
    player = {"kind": "function", "name": "play_fanfare", "params": [{"name": "bank", "array": False}], "locals": [],
              "body": [{"kind": "native", "name": "play_se", "args": [var("bank"), {"kind": "int", "value": 0}]}]}
    caller = {"kind": "function", "name": "on_start", "params": [], "locals": [{"name": "id"}],
              "body": [{"kind": "call", "name": "load_fanfare", "args": [{"kind": "addr", "target": var("id")}]},
                       {"kind": "call", "name": "play_fanfare", "args": [var("id")]}]}
    known = sdk.program_sounds([loader, player, caller], found)
    shown = sdk.named_sounds(player["body"], found, sdk.sound_banks(player, known), sdk.sound_flow(player, known))
    assert shown[0]["args"][1] == var("SE_SP_000_FANFARE")
    # a caller that passes something the pass cannot follow leaves the parameter unsettled
    other = {"kind": "function", "name": "on_end", "params": [], "locals": [{"name": "id"}],
             "body": [{"kind": "expr", "expr": {"kind": "assign", "target": var("id"), "value": {"kind": "call", "name": "get", "args": []}}},
                      {"kind": "call", "name": "play_fanfare", "args": [var("id")]}]}
    known = sdk.program_sounds([loader, player, caller, other], found)
    assert ("play_fanfare", 0) not in known["params"]


def test_banks_after_an_if_are_those_its_paths_agree_on():
    found = {"enums": {"se_sp_000_pal": {0: "SE_SP_000_FANFARE"}}, "constants": {}, "sound_set_banks": {}}
    var = lambda name: {"kind": "var", "name": name}
    load = lambda path: {"kind": "native", "name": "load_sound",
                         "args": [var("SND_PORT_MENU"), {"kind": "string", "text": path}], "result": var("id")}
    play = {"kind": "native", "name": "play_se", "args": [var("id"), {"kind": "int", "value": 0}]}
    branch = lambda otherwise: {"kind": "if", "cond": var("x"), "then": [load("snd2/sp/sp_000.snd")], "else": otherwise}
    # only one path loads: on the other the id holds no bank
    assert sdk.named_sounds([branch(None), play], found, {})[1]["args"][1] == var("SE_SP_000_FANFARE")
    # the paths load different banks
    assert sdk.named_sounds([branch([load("snd2/sp/sp_001.snd")]), play], found, {})[1]["args"][1]["kind"] == "int"
    # the path that loads the other bank returns
    returning = [load("snd2/sp/sp_001.snd"), {"kind": "return", "value": None}]
    assert sdk.named_sounds([branch(returning), play], found, {})[1]["args"][1] == var("SE_SP_000_FANFARE")



def test_locals_called_unused_that_are_read_get_a_true_name():
    found = {"by_original": {"_SET_MODE": {"name": "set_mode", "params": [{"name": "mode", "out": False}]}},
             "enums": {}, "constants": {}}
    var = lambda name: {"kind": "var", "name": name}
    body = [{"kind": "native", "name": "_SET_MODE", "base": None, "args": [var("unused2")]},
            {"kind": "expr", "expr": {"kind": "assign", "target": var("unused3"), "value": {"kind": "int", "value": 1}}},
            {"kind": "native", "name": "_SET_MODE", "base": None, "args": [var("unused3")]},
            {"kind": "expr", "expr": {"kind": "assign", "target": var("unused4"), "value": {"kind": "int", "value": 1}}}]
    function = {"params": [], "locals": [{"name": name, "type": "int", "size": None} for name in ("unused2", "unused3", "unused4")],
                "body": body}
    found = dict(found, natives={}, inlines={}, programs={})
    renamed = [declared["name"] for declared in sdk.named_locals(function, found)["locals"]]
    # read and never written; written and read; written only (still unused)
    assert renamed == ["unset_mode", "mode", "unused4"]
    # nor do they take the name of one of the script's functions
    renamed = [declared["name"] for declared in sdk.named_locals(function, found, {"mode"})["locals"]]
    assert renamed == ["unset_mode", "mode_2", "unused4"]
