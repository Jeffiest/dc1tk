import hashlib
import json
from pathlib import Path

def string_key(text):
    return hashlib.sha1(text.encode("utf-8")).hexdigest()[:16]


def load(directory, language="en"):
    path = Path(directory) / (language + ".json")
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))["strings"]


def strings_in(node, out):
    if isinstance(node, list):
        for item in node:
            strings_in(item, out)
    elif isinstance(node, dict):
        if node.get("kind") == "string":
            out.append(node["text"])
        for value in node.values():
            strings_in(value, out)
    return out


def translated(texts, translations):
    found = []
    for text in texts:
        english = translations.get(string_key(text))
        if english is not None and english not in found:
            found.append(english)
    return "; ".join(found)


def annotated(statements, translations):
    out = []
    for node in statements:
        kind = node["kind"]
        if kind == "if":
            otherwise = None if node["else"] is None else annotated(node["else"], translations)
            node = dict(node, then=annotated(node["then"], translations), **{"else": otherwise})
        elif kind == "while":
            node = dict(node, body=annotated(node["body"], translations))
        else:
            comment = translated(strings_in(node, []), translations)
            if comment:
                node = dict(node, comment="; ".join(part for part in (comment, node.get("comment")) if part))
        out.append(node)
    return out


def sibling_note(node, siblings):
    found = []
    for text in strings_in(node, []):
        english = siblings.get(text)
        if english is not None and english != text:
            flat = " ".join(part.strip() for part in english.replace("<page>", "\n").split("\n") if part.strip())
            if flat and '"%s"' % flat not in found:
                found.append('"%s"' % flat)
    return "; ".join(found)


def with_siblings(statements, siblings):
    out = []
    for node in statements:
        kind = node["kind"]
        if kind == "if":
            otherwise = None if node["else"] is None else with_siblings(node["else"], siblings)
            node = dict(node, then=with_siblings(node["then"], siblings), **{"else": otherwise})
        elif kind == "while":
            node = dict(node, body=with_siblings(node["body"], siblings))
        else:
            comment = sibling_note(node, siblings)
            if comment:
                node = dict(node, comment="; ".join(part for part in (node.get("comment"), comment) if part))
        out.append(node)
    return out


def annotate_siblings(functions, siblings):
    if not siblings:
        return functions
    return [dict(function, body=with_siblings(function["body"], siblings)) if function["kind"] == "function" else function
            for function in functions]


def sibling_strings(strings, other):
    if len(strings) != len(other):
        return {}
    found = {}
    clashing = set()
    for text, english in zip(strings, other):
        if found.setdefault(text, english) != english:
            clashing.add(text)
    return {text: english for text, english in found.items() if text not in clashing and text != english}


def annotate_functions(functions, translations):
    if not translations:
        return functions
    return [dict(function, body=annotated(function["body"], translations)) if function["kind"] == "function" else function
            for function in functions]


def annotate(program, translations):
    if "asm" in program:
        return program
    return dict(program, functions=annotate_functions(program["functions"], translations))
