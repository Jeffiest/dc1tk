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


def test_native_results_cannot_be_used_in_expressions():
    for broken in (EVENT.replace("flag = get_flag(12);", "flag = get_flag(12) + 1;"),
                   EVENT.replace("flag = get_flag(12);", "flag = set_actor_texture_anim(1, 0);")):
        try:
            build(broken)
        except ValueError:
            continue
        raise AssertionError("compiled " + broken)


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
