import hashlib
import json
from pathlib import Path

from c2s import index, pathnames, source

# A decompiled tree records what the decompiler wrote, so the tools can tell your edits from its
# output: setup and decompile refuse to overwrite edited files, changes lists them, build says which
# edited libraries reach scripts you did not touch, and mod save copies them into a mod (mods.py).

ORIGINALS = "original-hashes.json"


# Line endings do not count: an editor that rewrites them has not edited the file.
def file_hash(path):
    return hashlib.sha256(Path(path).read_bytes().replace(b"\r\n", b"\n")).hexdigest()


# Files of the tree, leaving out the record itself and hidden files and folders (clangd keeps its
# index in .cache/ beside compile_flags.txt).
def tracked(tree):
    root = Path(tree)
    found = []
    for path in root.rglob("*"):
        parts = path.relative_to(root).parts
        if path.is_file() and path.name not in (ORIGINALS, pathnames.UNNAMED) and not any(part.startswith(".") or part == "__pycache__" for part in parts):
            found.append(path.relative_to(root).as_posix())
    return sorted(found)


def originals(tree):
    path = Path(tree) / ORIGINALS
    return json.loads(path.read_text(encoding="utf-8"))["files"] if path.is_file() else None


# Records the files just written, keeping the record of any others (a folder decompiled into a
# tree that holds more).
def record(tree, paths):
    root = Path(tree)
    files = dict(originals(tree) or {})
    files.update({path: file_hash(root / path) for path in paths})
    files = dict(sorted(files.items()))
    note = "What the decompiler wrote, so the tools can tell edits from its output. Do not edit."
    (root / ORIGINALS).write_text(json.dumps({"note": note, "files": files}, indent=1) + "\n", encoding="utf-8")


def status(tree):
    root = Path(tree)
    recorded = originals(tree)
    if recorded is None:
        return None
    present = set(tracked(tree))
    return {"edited": sorted(path for path in recorded if path in present and file_hash(root / path) != recorded[path]),
            "added": sorted(present - set(recorded)),
            "removed": sorted(path for path in recorded if path not in present)}


# The files a decompile into tree would overwrite that hold edits (or that it cannot vouch for,
# when the tree has no record of what was written).
def overwrites(tree, paths):
    root = Path(tree)
    existing = [path for path in paths if (root / path).is_file()]
    found = status(tree)
    if found is None:
        return existing
    changed = set(found["edited"]) | set(found["added"])
    return [path for path in existing if path in changed]


def library_notes(tree, found):
    libraries = [path for path in found["edited"] + found["added"] if index.is_library(path)]
    if not libraries:
        return []
    users = index.includers(index.outlines(tree))
    touched = set(found["edited"]) | set(found["added"])
    lines = []
    for path in libraries:
        scripts = users.get(path, [])
        others = [script for script in scripts if script not in touched]
        lines.append("edited library %s is included by %d script%s%s" % (
            path, len(scripts), "" if len(scripts) == 1 else "s",
            ", %d of which you did not edit" % len(others) if others else ""))
    return lines


def changes_text(tree):
    found = status(tree)
    if found is None:
        return "%s has no %s, so edits cannot be told apart from decompiler output" % (tree, ORIGINALS)
    lines = []
    for label in ("edited", "added", "removed"):
        lines += ["%-8s %s" % (label, path) for path in found[label]]
    if not lines:
        return "no changes since the tree was decompiled"
    notes = library_notes(tree, found)
    return "\n".join(lines + ([""] + notes if notes else []))


# Gives some scripts their own copy of a library, so an edit to it reaches only them.
def fork(tree, library, copy, scripts):
    root = Path(tree)
    if not (root / library).is_file():
        raise SystemExit("no library %s in %s" % (library, tree))
    if (root / copy).exists():
        raise SystemExit("%s already exists" % copy)
    if not copy.startswith("lib/") or not copy.endswith(source.EXTENSION):
        raise SystemExit("the copy must be a lib/...%s path" % source.EXTENSION)
    line = '#include "%s"' % library
    missing = [script for script in scripts
               if line not in (root / script).read_text(encoding="utf-8").split("\n")]
    if missing:
        raise SystemExit("these scripts do not include %s directly:\n  %s" % (library, "\n  ".join(missing)))
    text = (root / library).read_text(encoding="utf-8")
    header = "// Copy of %s for %s, made by cli.py fork. Other scripts still include the original.\n\n" % (
        library, ", ".join(scripts))
    (root / copy).parent.mkdir(parents=True, exist_ok=True)
    (root / copy).write_text(header + text, encoding="utf-8")
    for script in scripts:
        path = root / script
        lines = path.read_text(encoding="utf-8").split("\n")
        path.write_text("\n".join('#include "%s"' % copy if item == line else item for item in lines), encoding="utf-8")
    users = index.includers(index.outlines(tree)).get(library, [])
    return {"still_using_original": users}
