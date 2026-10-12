import re

from c2s import asm, notes

# English names for the game's asset names. Scripts pick motions, texture animations, effects, map
# parts and damage settings by name, and the game's files name them in Japanese ("立ち" is the
# standing motion), so a script has to pass those exact strings. A whole-disc decompile writes a
# #define for each one the scripts pass to such a parameter into disc.h, named from the definition
# set's English gloss (MOTION_STAND), and the scripts use the names. The Japanese comes from the
# disc and the English from the set, as with the item and monster names in disc.h.

# The kind of asset a native parameter takes, by the parameter's name in the SDK.
PREFIXES = {
    "motion_name": "MOTION", "talk_motion": "MOTION", "idle_motion": "MOTION",
    "anim_name": "ANIM", "talk_anim": "ANIM", "idle_anim": "ANIM",
    "effect_name": "EFFECT", "effect": "EFFECT",
    "part": "PART",
    "hit_name": "HIT", "damage_name": "HIT",
    "texture_name": "TEXTURE",
}
JAPANESE = re.compile(r"[぀-ヿ一-鿿＀-￯]")
MOST_WORDS = 6


def asset_args(node, found):
    declared = found["by_original"].get(node["name"])
    if declared is None:
        return
    for position, arg in enumerate(node["args"]):
        if position >= len(declared["params"]) or arg["kind"] != "string":
            continue
        prefix = PREFIXES.get(declared["params"][position]["name"])
        if prefix and JAPANESE.search(arg["text"]) and "/" not in arg["text"]:
            yield position, prefix, arg["text"]


def walk(nodes, found, out):
    for node in nodes:
        if node["kind"] == "native":
            for _, prefix, text in asset_args(node, found):
                out.add((prefix, text))
        for key in ("then", "else", "body"):
            if isinstance(node.get(key), list):
                walk(node[key], found, out)


# Adds the asset names a program passes to {(prefix, text)}.
def survey(found_names, program, found):
    if found is None or "asm" in program:
        return
    for function in program["functions"]:
        if function["kind"] == "function":
            walk(function["body"], found, found_names)


def identifier(gloss):
    words = re.sub(r"[^A-Za-z0-9]+", " ", gloss).split()[:MOST_WORDS]
    return "_".join(words).upper()


# {(prefix, text): name} for the asset names that have a gloss. Two names that would read the same
# are numbered in the order of their Japanese text, so a disc always gets the same names.
def names(found_names, glosses, taken=()):
    out, used = {}, set(taken)
    for prefix, text in sorted(found_names):
        gloss = glosses.get(notes.string_key(text))
        if not gloss or not identifier(gloss):
            continue
        name = base = "%s_%s" % (prefix, identifier(gloss))
        copy = 1
        while name in used:
            copy += 1
            name = "%s_%d" % (base, copy)
        used.add(name)
        out[(prefix, text)] = name
    return out


def header_text(asset_names):
    lines = ["", "// English names for the game's asset names (motions, texture animations, effects, map parts,",
             "// damage settings), which the game's files name in Japanese."]
    for (prefix, text), name in sorted(asset_names.items(), key=lambda item: item[1]):
        lines.append("#define %s %s" % (name, asm.quote(text)))
    return "\n".join(lines) + "\n"


def with_header(generated, asset_names):
    if generated is None or not asset_names:
        return generated
    position = generated.rindex("#endif")
    return generated[:position].rstrip("\n") + "\n" + header_text(asset_names) + "\n" + generated[position:]


def in_context(context, asset_names):
    if context is None or not asset_names:
        return context
    return dict(context, asset_names=asset_names)


# Puts the names in for the asset strings a function passes to natives (before the natives are
# shown with their SDK names, while their arguments still line up with the declarations).
def named(nodes, found):
    asset_names = found.get("asset_names")
    if not asset_names:
        return nodes
    out = []
    for node in nodes:
        if node["kind"] == "native":
            args = list(node["args"])
            for position, prefix, text in asset_args(node, found):
                name = asset_names.get((prefix, text))
                if name:
                    args[position] = {"kind": "var", "name": name}
            node = dict(node, args=args)
        if node["kind"] == "if":
            node = dict(node, then=named(node["then"], found), **{"else": None if node["else"] is None else named(node["else"], found)})
        elif node["kind"] == "while":
            node = dict(node, body=named(node["body"], found))
        out.append(node)
    return out
