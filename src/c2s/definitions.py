import json
import re
from pathlib import Path

from c2s import names, notes, sdk, source

DEFAULT_SET = Path(__file__).resolve().parents[2] / "definitions" / "pal"
MANIFEST = "definitions.json"


def empty():
    return {"sets": [], "names": {}, "strings": {}, "tables": [], "language": "en", "include": [], "scripts": {}, "executable": {},
            "text": {}, "libraries": {}, "paths": {}}


def manifest(directory):
    path = Path(directory) / MANIFEST
    if not path.is_file():
        return {}
    return json.loads(path.read_text(encoding="utf-8"))


def load_scripts(directory):
    directory = Path(directory)
    if not directory.is_dir():
        return {}
    found = {}
    for path in sorted(directory.rglob("*.json")):
        disc_path = path.relative_to(directory).with_suffix(".stb").as_posix()
        found[disc_path] = json.loads(path.read_text(encoding="utf-8"))
    return found


VARIANT = re.compile(r"_\d\.stb$")


def variant_base(path):
    return VARIANT.sub(".stb", path)


def script_entry(defs, path):
    text = Path(path).as_posix()
    if text.endswith(source.EXTENSION):
        text = text[:-len(source.EXTENSION)] + ".stb"
    sibling = {}
    for disc_path, entry in defs["scripts"].items():
        if text == disc_path or text.endswith("/" + disc_path):
            return entry
        base = variant_base(disc_path)
        if not sibling and (variant_base(text) == base or variant_base(text).endswith("/" + base)):
            sibling = entry
    return sibling


def ordered_sets(directory, out):
    directory = Path(directory).resolve()
    if directory in out:
        return out
    for parent in manifest(directory).get("extends", []):
        ordered_sets(directory / parent, out)
    out.append(directory)
    return out


def load(directories):
    found = empty()
    order = []
    for directory in directories:
        ordered_sets(directory, order)
    for directory in order:
        settings = manifest(directory)
        found["sets"].append(str(directory))
        found["language"] = settings.get("language", found["language"])
        found["executable"] = dict(found["executable"], **settings.get("executable", {}))
        for key, value in settings.get("text", {}).items():
            found["text"][key] = dict(found["text"].get(key, {}), **value) if isinstance(value, dict) else value
        found["tables"] = [[rule["prefix"], rule["table"]] for rule in settings.get("tables", [])] + found["tables"]
        found["include"] += [name for name in settings.get("include", []) if name not in found["include"]]
        for table, entries in names.load(directory / "names").items():
            found["names"].setdefault(table, {}).update(entries)
        paths = directory / "paths.json"
        if paths.is_file():
            found["paths"].update(json.loads(paths.read_text(encoding="utf-8"))["names"])
        libraries = directory / "libraries.json"
        if libraries.is_file():
            for key, entry in json.loads(libraries.read_text(encoding="utf-8"))["libraries"].items():
                found["libraries"][key] = dict(found["libraries"].get(key, {}), **entry)
        for path, entry in load_scripts(directory / "scripts").items():
            found["scripts"][path] = dict(found["scripts"].get(path, {}), **entry)
    for directory in order:
        found["strings"].update(notes.load(directory / "strings", found["language"]))
    return found


def source_headers(path):
    texts = {}
    for folder in reversed(Path(path).resolve().parents):
        if (folder / "include").is_dir():
            texts.update(sdk.load_texts(folder / "include"))
    return texts


def header_texts(defs, sdk_dir):
    texts = sdk.load_texts(sdk_dir)
    for directory in defs["sets"]:
        include = Path(directory) / "include"
        if include.is_dir():
            texts.update(sdk.load_texts(include))
    return texts
